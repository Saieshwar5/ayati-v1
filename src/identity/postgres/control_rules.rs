//! An agent's tools: which connectors it uses and its own rules, which can only be
//! stricter than the workspace rule (D231 slice 4, D233 Tools). A change moves that
//! action's effective revision forward and fences its pending approvals.
use super::*;
use crate::identity::control::model::{AgentConnection, Rule};
use serde_json::json;

/// allow < ask < block.
pub(super) fn strictness(mode: &str) -> u8 {
    match mode {
        "allow" => 0,
        "ask" => 1,
        _ => 2,
    }
}

impl PgStore {
    pub(crate) async fn control_disconnect(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        ensure!(execute(&tx,"UPDATE relay_control_connections SET enabled=false,revision=revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?==1,"Connection unavailable");
        super::workspace_connections::reset_connection(&tx, w, id).await?;
        audit(&tx, &user.id, Some(w), "control_disconnected", id).await?;
        tx.commit().await?;
        Ok(())
    }
    /// The Tools tab: each approved connector, whether the agent uses it, and for
    /// every action the workspace rule, the agent's own rule and the result.
    pub(crate) async fn control_snapshot(
        &self,
        user: &User,
        w: &str,
        agent: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let profile = super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let connections = json_rows(
            &tx,
            include_str!("agent_tools.sql"),
            vec![w.into(), agent.into()],
        )
        .await?;
        let actions = json_rows(&tx,&format!("SELECT {VIEW} AS data FROM relay_control_actions a JOIN relay_chats c ON c.workspace=a.workspace AND c.id=a.chat WHERE a.workspace=$1 AND a.agent=$2 AND (a.requester=$3 OR NOT EXISTS(SELECT 1 FROM relay_library_sources l WHERE l.workspace=a.workspace AND l.reply=a.job)) ORDER BY a.created DESC,a.id DESC LIMIT 50"),vec![w.into(),agent.into(),user.id.clone().into()]).await?;
        let mut visible_actions = vec![];
        for action in actions {
            let record =
                super::approval_delivery::action(&tx, w, action["id"].as_str().unwrap()).await?;
            if super::approval_delivery::viewer(&tx, user, w, &record)
                .await
                .is_ok()
            {
                visible_actions.push(action);
            }
        }
        tx.commit().await?;
        Ok(json!({
            "connections": connections,
            "actions": visible_actions,
            "can_manage": admin || profile["owner_id"] == user.id,
            "can_manage_connections": admin,
            "viewer": user.id,
        }))
    }
    pub(crate) async fn control_connect(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        definition: Value,
        fingerprint: &str,
    ) -> Result<()> {
        crate::identity::worker_control::model::uuid(id)?;
        let server = definition["id"]
            .as_str()
            .context("Missing server identity")?;
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        super::workspace_connections::approved(&tx, w, server).await?;
        if let Some(row) = query(&tx,"SELECT id,server,fingerprint,enabled FROM relay_control_connections WHERE workspace=$1 AND (id=$2 OR server=$3)",vec![w.into(),id.into(),server.into()]).await? {
            ensure!(row.try_get::<String>("","id")? == id && row.try_get::<String>("","server")? == server,"Connection already exists; reload before connecting");
            if row.try_get::<String>("","fingerprint")? != fingerprint || !row.try_get::<bool>("","enabled")? {
                execute(&tx,"UPDATE relay_control_connections SET definition=$3,fingerprint=$4,enabled=true,revision=revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),definition.into(),fingerprint.into()]).await?;
                super::workspace_connections::reset_connection(&tx,w,id).await?;
            }
        } else {
            execute(&tx,"INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint) VALUES($1,$2,$3,$4,$5)",vec![w.into(),id.into(),server.into(),definition.into(),fingerprint.into()]).await?;
        }
        audit(&tx, &user.id, Some(w), "control_connected", id).await?;
        tx.commit().await?;
        Ok(())
    }
    /// Switches a connector on or off for an agent. Off stops its pending approvals.
    pub(crate) async fn control_agent_connection(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &AgentConnection,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        if input.enabled {
            available(&tx, w, &input.connection).await?;
            execute(&tx,"INSERT INTO relay_agent_connections(workspace,agent,connection,revision,enabled_by,at) VALUES($1,$2,$3,nextval('relay_control_revision'),$4,$5) ON CONFLICT DO NOTHING",vec![w.into(),agent.into(),input.connection.clone().into(),user.id.clone().into(),now().into()]).await?;
        } else {
            execute(&tx,"DELETE FROM relay_agent_connections WHERE workspace=$1 AND agent=$2 AND connection=$3",vec![w.into(),agent.into(),input.connection.clone().into()]).await?;
            invalidate(&tx, w, &input.connection, Some(agent), None).await?;
        }
        audit(
            &tx,
            &user.id,
            Some(w),
            if input.enabled {
                "control_agent_connection_on"
            } else {
                "control_agent_connection_off"
            },
            &format!("{agent}/{}", input.connection),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    /// Saves the agent's own rule for one action. It may only be as strict as the
    /// workspace rule or stricter.
    pub(crate) async fn control_rule(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &Rule,
    ) -> Result<()> {
        ensure!(
            ["allow", "ask", "block"].contains(&input.mode.as_str())
                && (0..i64::MAX).contains(&input.revision),
            "Invalid permission rule"
        );
        let tx = self.workspace_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        ensure!(query(&tx,"SELECT agent FROM relay_agent_connections WHERE workspace=$1 AND agent=$2 AND connection=$3",vec![w.into(),agent.into(),input.connection.clone().into()]).await?.is_some(),"Switch this connector on for the agent first");
        let ceiling =
            super::connector_rules::workspace_mode(&tx, w, &input.connection, &input.operation)
                .await?;
        ensure!(
            strictness(&input.mode) >= strictness(&ceiling),
            "Your admin set this action to {}; an agent can only be stricter",
            super::connector_rules::label(&ceiling)
        );
        let old = query(&tx,"SELECT revision,mode FROM relay_control_rules WHERE workspace=$1 AND agent=$2 AND connection=$3 AND operation=$4",vec![w.into(),agent.into(),input.connection.clone().into(),input.operation.clone().into()]).await?;
        let actual = old
            .as_ref()
            .map(|r| r.try_get::<i64>("", "revision"))
            .transpose()?
            .unwrap_or(0);
        let same = old
            .as_ref()
            .map(|r| r.try_get::<String>("", "mode"))
            .transpose()?
            .as_deref()
            == Some(input.mode.as_str());
        if super::workspace_connections::stale_revision(actual, input.revision, same)? {
            tx.commit().await?;
            return Ok(());
        }
        execute(&tx,"INSERT INTO relay_control_rules(workspace,agent,connection,operation,mode,configured_by,revision,changed) VALUES($1,$2,$3,$4,$5,$6,1,nextval('relay_control_revision')) ON CONFLICT(workspace,agent,connection,operation) DO UPDATE SET mode=excluded.mode,configured_by=excluded.configured_by,revision=relay_control_rules.revision+1,changed=excluded.changed",vec![w.into(),agent.into(),input.connection.clone().into(),input.operation.clone().into(),input.mode.clone().into(),user.id.clone().into()]).await?;
        invalidate(
            &tx,
            w,
            &input.connection,
            Some(agent),
            Some(&input.operation),
        )
        .await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            "control_rule_saved",
            &format!("{agent}/{}/{}", input.connection, input.operation),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    /// Whether the agent has any action it may take or ask for.
    pub(crate) async fn control_enabled(&self, w: &str, agent: &str) -> Result<bool> {
        Ok(query(&self.db,"SELECT agent FROM relay_control_effective_rules WHERE workspace=$1 AND agent=$2 LIMIT 1",vec![w.into(),agent.into()]).await?.is_some())
    }
}

/// The connector is connected, enabled and its system approved by an admin.
async fn available(db: &impl ConnectionTrait, w: &str, connection: &str) -> Result<()> {
    ensure!(query(db,"SELECT c.id FROM relay_control_connections c JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server WHERE c.workspace=$1 AND c.id=$2 AND c.enabled AND s.approved",vec![w.into(),connection.into()]).await?.is_some(),"Connection unavailable; ask an admin to connect it");
    Ok(())
}

pub(super) const VIEW: &str = "jsonb_build_object('id',a.id,'connection',a.connection,'operation',a.operation,'title',a.title,'effect',a.effect,'decision',a.decision,'requester_name',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=a.requester),'conversation_name',(SELECT c.name FROM relay_chats c WHERE c.workspace=a.workspace AND c.id=a.chat),'delivery',(SELECT jsonb_build_object('status',d.status,'reason',d.reason) FROM relay_control_deliveries d WHERE d.workspace=a.workspace AND d.action=a.id),'arguments',a.arguments,'state',CASE WHEN a.state='executing' AND a.updated < floor(extract(epoch FROM clock_timestamp()))::bigint-60 THEN 'uncertain' ELSE a.state END,'result',a.result,'chat',a.chat,'thread',a.thread,'job',a.job,'approver',a.approver,'created',a.created,'updated',a.updated)";
/// Pending approvals for these rules ask again after a change.
pub(super) async fn invalidate(
    db: &impl ConnectionTrait,
    w: &str,
    connection: &str,
    agent: Option<&str>,
    operation: Option<&str>,
) -> Result<()> {
    execute(db,"UPDATE relay_control_actions SET state='invalidated',result='\"Permissions changed; request a new action.\"'::jsonb,updated=floor(extract(epoch FROM clock_timestamp()))::bigint WHERE workspace=$1 AND connection=$2 AND state='pending' AND ($3::text IS NULL OR agent=$3) AND ($4::text IS NULL OR operation=$4)",vec![w.into(),connection.into(),agent.map(str::to_owned).into(),operation.map(str::to_owned).into()]).await?;
    Ok(())
}
