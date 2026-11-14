//! App events become agent work (Apps AP10). The app's builder or an admin asks an
//! agent to act when a record is added to a collection, or changed ("tell me when a leave request
//! comes in"); each new live record is queued once and handled, one at a time per
//! collection, in that person's conversation with the agent and as that person — so in
//! the app the agent can never do more than they could. Records the watching agent
//! made itself don't start more work.
use super::*;
use crate::identity::built_apps::manifest::Manifest;
use crate::identity::chat::model::{NewChat, NewMessage};
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;
use serde_json::json;

/// Schema 84: app event watchers and the events waiting for them.
pub(crate) struct BuiltAppEvents;
impl MigrationName for BuiltAppEvents {
    fn name(&self) -> &str {
        "m20261105_000084_built_app_events"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for BuiltAppEvents {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("built_app_events.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Built app events require a forward migration".into(),
        ))
    }
}

fn added() -> String {
    "added".into()
}
/// Schema 86: watchers for changed records, and preview errors (kept with the code).
pub(crate) struct BuiltAppChanges;
impl MigrationName for BuiltAppChanges {
    fn name(&self) -> &str {
        "m20261105_000086_built_app_changes"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for BuiltAppChanges {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("built_app_changes.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Built app changes require a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppWatch {
    pub collection: String,
    /// added (the default) or changed.
    #[serde(default = "added")]
    pub kind: String,
    /// The agent to act, or none to stop.
    pub agent: Option<String>,
    #[serde(default)]
    pub instructions: String,
}

/// One live record that was added, or changed to a revision, in an app's collection.
pub(super) struct Happened<'a> {
    pub app: &'a str,
    pub collection: &'a str,
    pub record: &'a str,
    /// added or changed.
    pub kind: &'a str,
    pub revision: i64,
    /// For a change: the fields set, with their new values.
    pub changes: Option<Value>,
}

/// Queues what happened for the collection's watcher of that kind, unless it did it.
pub(super) async fn queue(
    tx: &DatabaseTransaction,
    w: &str,
    event: Happened<'_>,
    agent: Option<&str>,
) -> Result<()> {
    execute(tx, "INSERT INTO relay_built_app_events(workspace,app,collection,record,kind,revision,changes,at) SELECT x.workspace,x.app,x.collection,$4,x.kind,$7,$8,$9 FROM relay_built_app_watchers x WHERE x.workspace=$1 AND x.app=$2 AND x.collection=$3 AND x.kind=$6 AND x.agent IS DISTINCT FROM $5 ON CONFLICT DO NOTHING", vec![w.into(), event.app.into(), event.collection.into(), event.record.into(), agent.map(str::to_owned).into(), event.kind.into(), event.revision.into(), event.changes.into(), now().into()]).await?;
    Ok(())
}

/// A queued event: workspace, app, collection, record, kind and revision.
type Key = (String, String, String, String, String, i64);
async fn published_manifest(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
    version: i32,
) -> Result<Option<Manifest>> {
    let row = query(tx, "SELECT content FROM relay_built_app_files WHERE workspace=$1 AND app=$2 AND version=$3 AND path='cuckoo-app.json'", vec![w.into(), id.into(), version.into()]).await?;
    Ok(row
        .and_then(|r| r.try_get::<String>("", "content").ok())
        .and_then(|t| Manifest::parse(&t).ok()))
}
async fn skip(tx: DatabaseTransaction, key: &Key, detail: &str) -> Result<bool> {
    execute(&tx, "UPDATE relay_built_app_events SET state='skipped',detail=$7 WHERE workspace=$1 AND app=$2 AND collection=$3 AND record=$4 AND kind=$5 AND revision=$6 AND state='pending'", vec![key.0.clone().into(), key.1.clone().into(), key.2.clone().into(), key.3.clone().into(), key.4.clone().into(), key.5.into(), detail.into()]).await?;
    tx.commit().await?;
    Ok(false)
}

/// The app triggers an agent acts on (Agent › Automations), in apps the viewer may
/// open or manage; changing one stays with the app's builder or an admin, in the app.
pub(crate) async fn agent_app_triggers(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
) -> Result<Vec<Value>> {
    app_triggers_for(tx, user, w, &[agent.to_owned()]).await
}

/// The app triggers of these agents (Activity › Upcoming), as `agent_app_triggers`.
pub(crate) async fn app_triggers_for(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agents: &[String],
) -> Result<Vec<Value>> {
    let present: bool = query(
        tx,
        "SELECT to_regclass('relay_built_app_watchers') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Schema check")?
    .try_get("", "present")?;
    if !present || agents.is_empty() {
        return Ok(vec![]);
    }
    let admin = role(tx, w, &user.id).await? == "Admin";
    json_rows(tx, &format!("SELECT jsonb_build_object('agent',x.agent,'agent_name',g.name,'app',a.id,'app_name',a.name,'collection',x.collection,'kind',x.kind,'instructions',x.instructions,'by',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=x.principal),'manages',a.builder=$2 OR $3) AS data FROM relay_built_app_watchers x JOIN relay_built_apps a ON a.workspace=x.workspace AND a.id=x.app JOIN relay_shared_agents g ON g.workspace=x.workspace AND g.id=x.agent WHERE x.workspace=$1 AND x.agent IN (SELECT jsonb_array_elements_text($4::jsonb)) AND NOT a.removed AND ({OPENS} OR a.builder=$2 OR $3) ORDER BY a.name,x.collection,x.kind"), vec![w.into(), user.id.clone().into(), admin.into(), serde_json::json!(agents).into()]).await
}

impl PgStore {
    /// The app's collections and who acts on new records in each.
    pub(crate) async fn built_app_watchers(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let (found, _) = super::app(&tx, user, w, id).await?;
        let version = found["published"].as_i64().unwrap_or(0) as i32;
        let collections: Vec<String> = published_manifest(&tx, w, id, version)
            .await?
            .map(|m| m.collections.into_keys().collect())
            .unwrap_or_default();
        let watchers = json_rows(&tx, "SELECT jsonb_build_object('collection',x.collection,'kind',x.kind,'agent',x.agent,'agent_name',g.name,'instructions',x.instructions,'by',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=x.principal),'chat',CASE WHEN x.principal=$3 THEN x.chat END,'waiting',(SELECT count(*) FROM relay_built_app_events e WHERE e.workspace=x.workspace AND e.app=x.app AND e.collection=x.collection AND e.kind=x.kind AND e.state='pending')) AS data FROM relay_built_app_watchers x JOIN relay_shared_agents g ON g.workspace=x.workspace AND g.id=x.agent WHERE x.workspace=$1 AND x.app=$2 ORDER BY x.collection,x.kind", vec![w.into(), id.into(), user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(json!({"collections":collections,"watchers":watchers,"can_change":found["manages"]}))
    }

    /// The builder or an admin asks an agent to act on new records in a collection, or
    /// stops it. The work goes into a conversation of their own with the agent.
    pub(crate) async fn built_app_watch_set(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &AppWatch,
    ) -> Result<Value> {
        let (found, collections) = {
            let tx = self.read_tx(w).await?;
            let (found, _) = super::app(&tx, user, w, id).await?;
            let version = found["published"].as_i64().unwrap_or(0) as i32;
            let manifest = published_manifest(&tx, w, id, version).await?;
            tx.commit().await?;
            (
                found,
                manifest
                    .map(|m| m.collections.into_keys().collect::<Vec<_>>())
                    .unwrap_or_default(),
            )
        };
        ensure!(
            found["manages"] == true,
            "Its builder or an admin decides what agents do in an app"
        );
        ensure!(found["published"].is_i64(), "Publish the app first");
        ensure!(
            ["added", "changed"].contains(&input.kind.as_str()),
            "Act on added or changed records"
        );
        let what = if input.kind == "added" {
            "new"
        } else {
            "changed"
        };
        ensure!(
            collections.contains(&input.collection),
            "The app has no collection \"{}\"",
            input.collection
        );
        let name = found["name"].as_str().unwrap_or("").to_owned();
        let Some(agent) = &input.agent else {
            let tx = self.workspace_tx(w).await?;
            execute(&tx, "DELETE FROM relay_built_app_watchers WHERE workspace=$1 AND app=$2 AND collection=$3 AND kind=$4", vec![w.into(), id.into(), input.collection.clone().into(), input.kind.clone().into()]).await?;
            execute(&tx, "UPDATE relay_built_app_events SET state='skipped',detail='Nobody acts on these any more.' WHERE workspace=$1 AND app=$2 AND collection=$3 AND kind=$4 AND state='pending'", vec![w.into(), id.into(), input.collection.clone().into(), input.kind.clone().into()]).await?;
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "built_app_watch",
                id,
                &format!(
                    "Stopped agents acting on {what} {} in \"{name}\"",
                    input.collection
                ),
            )
            .await?;
            tx.commit().await?;
            return Ok(json!({"ok":true}));
        };
        let instructions = input.instructions.trim();
        ensure!(
            !instructions.is_empty() && instructions.chars().count() <= 2000,
            "Say what the agent should do, in up to 2,000 characters"
        );
        let current = {
            let tx = self.read_tx(w).await?;
            let row = query(&tx, "SELECT chat FROM relay_built_app_watchers WHERE workspace=$1 AND app=$2 AND agent=$3 AND principal=$4 LIMIT 1", vec![w.into(), id.into(), agent.clone().into(), user.id.clone().into()]).await?;
            tx.commit().await?;
            row.map(|r| r.try_get::<String>("", "chat")).transpose()?
        };
        // Starting the conversation also checks this person may work with the agent.
        let chat = match current {
            Some(chat) => chat,
            None => {
                self.chat_create(
                    user,
                    w,
                    &NewChat {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: format!("{name} (app)"),
                        users: vec![],
                        agents: vec![agent.clone()],
                        kind: "group".into(),
                        description: "What happens in this app is handled here.".into(),
                    },
                )
                .await?
            }
        };
        let tx = self.workspace_tx(w).await?;
        execute(&tx, "INSERT INTO relay_built_app_watchers(workspace,app,collection,agent,principal,chat,instructions,set_at,kind) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(workspace,app,collection,kind) DO UPDATE SET agent=EXCLUDED.agent,principal=EXCLUDED.principal,chat=EXCLUDED.chat,instructions=EXCLUDED.instructions,set_at=EXCLUDED.set_at", vec![w.into(), id.into(), input.collection.clone().into(), agent.clone().into(), user.id.clone().into(), chat.clone().into(), instructions.into(), now().into(), input.kind.clone().into()]).await?;
        let agent_name: String = query(
            &tx,
            "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
            vec![w.into(), agent.clone().into()],
        )
        .await?
        .context("Choose an agent in this workspace")?
        .try_get("", "name")?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "built_app_watch",
            id,
            &format!(
                "Asked {agent_name} to act on {what} {} in \"{name}\"",
                input.collection
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"ok":true,"chat":chat}))
    }

    /// Starts agent work for new app records, one at a time per collection.
    pub(crate) async fn dispatch_app_events(&self) -> Result<usize> {
        if !self.pooled
            || query(
                &self.db,
                "SELECT to_regclass('relay_built_app_events') IS NOT NULL AS present",
                vec![],
            )
            .await?
            .context("Schema check")?
            .try_get::<bool>("", "present")
            .ok()
                != Some(true)
        {
            return Ok(0);
        }
        let waiting = rows(&self.db, "SELECT e.workspace,e.app,e.collection,e.record,e.kind,e.revision FROM relay_built_app_events e WHERE e.state='pending' AND NOT EXISTS(SELECT 1 FROM relay_built_app_events b JOIN relay_chat_jobs j ON j.workspace=b.workspace AND j.reply=b.reply WHERE b.workspace=e.workspace AND b.app=e.app AND b.collection=e.collection AND b.state='started' AND j.state<>'finished') ORDER BY e.at,e.record,e.revision LIMIT 20", vec![]).await?;
        let (mut started, mut seen) = (0, std::collections::HashSet::new());
        for row in waiting {
            let key: Key = (
                row.try_get("", "workspace")?,
                row.try_get("", "app")?,
                row.try_get("", "collection")?,
                row.try_get("", "record")?,
                row.try_get("", "kind")?,
                row.try_get("", "revision")?,
            );
            if seen.insert((key.0.clone(), key.1.clone(), key.2.clone())) {
                started += usize::from(self.start_app_event(&key).await?);
            }
        }
        Ok(started)
    }

    async fn start_app_event(&self, key: &Key) -> Result<bool> {
        let (w, app, collection, record, kind) = (
            key.0.as_str(),
            key.1.as_str(),
            key.2.as_str(),
            key.3.as_str(),
            key.4.as_str(),
        );
        let tx = self.workspace_tx(w).await?;
        let Some(watcher) = json_rows(&tx, "SELECT to_jsonb(x) || jsonb_build_object('name',a.name,'published',a.published) AS data FROM relay_built_app_watchers x JOIN relay_built_apps a ON a.workspace=x.workspace AND a.id=x.app WHERE x.workspace=$1 AND x.app=$2 AND x.collection=$3 AND x.kind=$4 AND NOT a.removed", vec![w.into(), app.into(), collection.into(), kind.into()]).await?.into_iter().next() else {
            return skip(tx, key, "Nobody acts on these any more.").await;
        };
        if watcher["published"].is_null() {
            return skip(tx, key, "The app was taken offline.").await;
        }
        let Some(added) = json_rows(&tx, "SELECT jsonb_build_object('data',r.data,'by',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=CASE WHEN $5='changed' THEN r.updated_by ELSE r.created_by END),'changes',(SELECT e.changes FROM relay_built_app_events e WHERE e.workspace=r.workspace AND e.app=r.app AND e.collection=r.collection AND e.record=r.id AND e.kind=$5 AND e.revision=$6),'agent',(SELECT g.name FROM relay_shared_agents g WHERE g.workspace=r.workspace AND g.id=r.by_agent)) AS data FROM relay_built_app_records r WHERE r.workspace=$1 AND r.app=$2 AND r.stage='live' AND r.collection=$3 AND r.id=$4", vec![w.into(), app.into(), collection.into(), record.into(), kind.into(), key.5.into()]).await?.into_iter().next() else {
            return skip(tx, key, "The record was removed before it was handled.").await;
        };
        let principal = query(
            &tx,
            "SELECT id,email,display_name FROM relay_users WHERE id=$1",
            vec![watcher["principal"].as_str().unwrap_or("").into()],
        )
        .await?
        .context("Watcher")?;
        let principal = User {
            id: principal.try_get("", "id")?,
            email: principal.try_get("", "email")?,
            display_name: principal.try_get("", "display_name")?,
        };
        let shown = if kind == "changed" {
            &added["changes"]
        } else {
            &added["data"]
        };
        let fields: Vec<String> = shown
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| {
                format!(
                    "{k}: {}",
                    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
                )
            })
            .collect();
        let via = added["agent"]
            .as_str()
            .map(|a| format!(" (through {a})"))
            .unwrap_or_default();
        let input = NewMessage {
            reply_to: None,
            id: uuid::Uuid::new_v4().to_string(),
            agent: watcher["agent"].as_str().map(str::to_owned),
            agents: vec![],
            thread: None,
            attachments: vec![],
            content: format!(
                "{} in the app \"{}\" ({collection}), {} by {}{via}: {}\n{}\nThe record's id is {record} in app {app}; use app_actions and run_app_action to act in the app.",
                if kind == "changed" { "Changed" } else { "New" },
                watcher["name"].as_str().unwrap_or(""),
                kind,
                added["by"].as_str().unwrap_or("someone"),
                fields.join(", "),
                watcher["instructions"].as_str().unwrap_or("")
            ),
            broadcast: false,
        };
        let nested = tx.begin().await?;
        let job = match self
            .background_in(
                &nested,
                &principal,
                w,
                watcher["chat"].as_str().unwrap_or(""),
                &input,
            )
            .await
        {
            Ok(job) => {
                nested.commit().await?;
                job.context("The app request was not accepted")?
            }
            Err(error) => {
                nested.rollback().await?;
                if error.is::<crate::identity::chat::model::QueueFull>()
                    || error.is::<crate::identity::chat::model::BackgroundQueueFull>()
                {
                    tx.commit().await?;
                    return Ok(false);
                }
                if error.is::<sea_orm::DbErr>() {
                    return Err(error);
                }
                let detail = if error.is::<crate::identity::chat::model::AgentsPaused>() {
                    "All agents were paused when this was added, so it was not handled."
                } else {
                    "The watcher can no longer ask this agent; it was not handled."
                };
                return skip(tx, key, detail).await;
            }
        };
        execute(&tx, "UPDATE relay_built_app_events SET state='started',reply=$5 WHERE workspace=$1 AND app=$2 AND collection=$3 AND record=$4 AND kind=$6 AND revision=$7 AND state='pending'", vec![w.into(), app.into(), collection.into(), record.into(), job.reply.clone().into(), kind.into(), key.5.into()]).await?;
        tx.commit().await?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "built_app_events_tests.rs"]
mod tests;
