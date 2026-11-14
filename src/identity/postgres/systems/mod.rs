//! Connected systems (schema 116, C1 of the connected systems plan, D249): an
//! agent's owner or an admin connects a company's own MCP server from the
//! agent's Tools tab, reviews each tool, and the agent uses it under its rule.
//! Later slices add events (C2), results (C4) and subjects (C3) to the same
//! connection.
use super::*;
use crate::identity::control::model::Operation;
use serde::{Deserialize, Serialize};
use serde_json::json;
pub(crate) mod automations;
mod connect;
pub(crate) mod events;
#[cfg(test)]
mod events_tests;
pub(crate) mod results;
pub(crate) mod subjects;
#[cfg(test)]
mod subjects_tests;
#[cfg(test)]
mod tests;
pub(crate) use migration::{ConnectedSystems, SystemEvents};
pub(crate) use subjects::Subjects;

/// One tool as the person reviewed it.
#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ToolChoice {
    pub name: String,
    /// `read` or `write`.
    pub kind: String,
    /// `allow`, `ask` or `block`.
    pub rule: String,
    /// "Needs the subject" (C3): the tool has a `subject` input that Cuckoo
    /// fills from the event and hides from the agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub subject: bool,
}

/// A request to connect a system to an agent, after the person reviewed its tools.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewSystem {
    /// The new connection's id, chosen by the browser so a repeat is the same request.
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub tools: Vec<ToolChoice>,
}
impl NewSystem {
    pub(crate) fn validate(&self) -> Result<()> {
        crate::identity::worker_control::model::uuid(&self.id)?;
        ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 100
                && !self.name.chars().any(char::is_control),
            "Give the system a name of up to 100 characters"
        );
        check_tools(&self.tools)
    }
    /// The same request again is the same connection; the key never goes in.
    pub(crate) fn request_hash(&self) -> Result<String> {
        let mut value = serde_json::to_value(self)?;
        value.as_object_mut().unwrap().remove("key");
        value.sort_all_objects();
        Ok(crate::identity::provider::digest(&value.to_string()))
    }
}

pub(crate) fn check_tools(tools: &[ToolChoice]) -> Result<()> {
    ensure!(
        !tools.is_empty() && tools.len() <= crate::identity::connectors::system_tools::MAX_ON,
        "Switch on between 1 and 20 tools"
    );
    let mut names = std::collections::HashSet::new();
    for tool in tools {
        ensure!(names.insert(&tool.name), "A tool is listed twice");
        ensure!(
            ["read", "write"].contains(&tool.kind.as_str())
                && ["allow", "ask", "block"].contains(&tool.rule.as_str()),
            "Choose Reads or Changes, and Allow, Ask first or Block, for each tool"
        );
    }
    Ok(())
}

/// A new review of a saved system's tools, at the system's current revision.
pub(crate) struct SystemReview {
    pub revision: i64,
    pub tools: Vec<ToolChoice>,
}

/// What the connection saves: the reviewed definition and the binding.
pub(crate) struct Reviewed {
    pub operations: Vec<Operation>,
    pub binding: Value,
}
impl Reviewed {
    pub(crate) fn definition(&self, server: &str, name: &str) -> Value {
        json!({"id":server,"name":name,"operations":self.operations})
    }
    pub(crate) fn fingerprint(&self, server: &str, name: &str) -> Result<String> {
        Ok(crate::identity::provider::digest(&format!(
            "{}\n{}",
            serde_json::to_string(&self.definition(server, name))?,
            serde_json::to_string(&self.binding)?
        )))
    }
}

/// Whether this schema has connected systems (116+).
pub(super) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_connected_systems') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// The agent's systems as its Tools tab shows them. Only field names and
/// states: never the address's key.
async fn tiles(db: &impl ConnectionTrait, w: &str, agent: &str) -> Result<Vec<Value>> {
    let mut tiles = json_rows(db, "SELECT jsonb_build_object('id',s.connection,'name',s.name,'tools',s.tools,'revision',s.revision,
            'enabled',c.enabled,'url',c.binding->>'url','has_key',c.binding->>'key' IS NOT NULL,'created_ms',s.created_ms,
            'created_by',COALESCE(u.display_name,u.email),'reviewed_ms',s.reviewed_ms,
            'events_on',COALESCE((to_jsonb(s)->>'events_on')::boolean,false),
            'last_call_ms',(SELECT max(k.started_ms) FROM relay_connector_calls k WHERE k.workspace=s.workspace AND k.app=c.server),
            'held',COALESCE((SELECT jsonb_agg(d.operation ORDER BY d.operation) FROM relay_connector_drift d
                WHERE d.workspace=s.workspace AND d.connection=s.connection AND d.fingerprint=c.fingerprint AND d.status IN ('missing','changed')
                AND d.run=(SELECT d2.run FROM relay_connector_drift d2 JOIN relay_connector_drift_runs r ON r.id=d2.run
                    WHERE d2.workspace=s.workspace AND d2.connection=s.connection ORDER BY r.started_ms DESC LIMIT 1)),'[]'::jsonb),
            'unreachable',COALESCE((SELECT bool_and(d.status='failed') FROM relay_connector_drift d
                WHERE d.workspace=s.workspace AND d.connection=s.connection AND d.fingerprint=c.fingerprint
                AND d.run=(SELECT d2.run FROM relay_connector_drift d2 JOIN relay_connector_drift_runs r ON r.id=d2.run
                    WHERE d2.workspace=s.workspace AND d2.connection=s.connection ORDER BY r.started_ms DESC LIMIT 1)),false)) AS data
        FROM relay_connected_systems s JOIN relay_control_connections c ON c.workspace=s.workspace AND c.id=s.connection
        LEFT JOIN relay_users u ON u.id=s.created_by
        WHERE s.workspace=$1 AND s.agent=$2 AND NOT s.removed ORDER BY s.created_ms,s.connection",
        vec![w.into(), agent.into()]).await?;
    for tile in &mut tiles {
        let state = if tile["enabled"] != true {
            "off"
        } else if tile["held"].as_array().is_some_and(|h| !h.is_empty()) {
            "changed"
        } else if tile["unreachable"] == true {
            "unreachable"
        } else {
            "connected"
        };
        tile["state"] = json!(state);
    }
    Ok(tiles)
}

impl PgStore {
    /// `GET …/agent-profiles/{agent}/systems`: anyone who may see the agent's
    /// tools sees its systems; only its owner and admins may change them.
    pub(crate) async fn agent_systems(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let profile = super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let systems = if ready(&tx).await? {
            tiles(&tx, w, agent).await?
        } else {
            vec![]
        };
        tx.commit().await?;
        Ok(
            json!({"systems":systems,"can_connect":admin || profile["owner_id"] == user.id.as_str()}),
        )
    }
}

mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct ConnectedSystems;
    impl MigrationName for ConnectedSystems {
        fn name(&self) -> &str {
            "m20261114_000116_connected_systems"
        }
    }
    pub(crate) struct SystemEvents;
    impl MigrationName for SystemEvents {
        fn name(&self) -> &str {
            "m20261114_000117_system_events"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for SystemEvents {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("events.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "System events require a forward migration".into(),
            ))
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for ConnectedSystems {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("systems.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Connected systems require a forward migration".into(),
            ))
        }
    }
}
