//! Test helper: gives an agent a connector action the way people do it now
//! (schema 47): an admin sets the workspace rule and switches the connector on.
use super::*;
use crate::identity::control::{connections_model::ConnectorRule, model::AgentConnection};

pub(super) async fn workspace_rule(
    db: &PgStore,
    admin: &User,
    w: &str,
    connection: &str,
    operation: &str,
    mode: &str,
) {
    let current = query(&db.db,"SELECT revision,mode FROM relay_connector_rules WHERE workspace=$1 AND connection=$2 AND operation=$3",vec![w.into(),connection.into(),operation.into()]).await.unwrap();
    let revision: i64 = current
        .as_ref()
        .map(|r| r.try_get("", "revision").unwrap())
        .unwrap_or(0);
    db.control_connector_rule(
        admin,
        w,
        &ConnectorRule {
            connection: connection.into(),
            operation: operation.into(),
            mode: mode.into(),
            needs_other: false,
            revision,
        },
    )
    .await
    .unwrap();
}

/// The workspace rule for `operation` becomes `mode`, and `agent` uses the connector.
pub(super) async fn grant(
    db: &PgStore,
    admin: &User,
    w: &str,
    agent: &str,
    connection: &str,
    operation: &str,
    mode: &str,
) {
    workspace_rule(db, admin, w, connection, operation, mode).await;
    db.control_agent_connection(
        admin,
        w,
        agent,
        &AgentConnection {
            connection: connection.into(),
            enabled: true,
        },
    )
    .await
    .unwrap();
}

/// Gives one agent its own mode for an action, the way older tests set a rule per
/// agent: the workspace rule allows it and the agent's own rule narrows it.
pub(super) async fn agent_rule(
    db: &PgStore,
    admin: &User,
    w: &str,
    agent: &str,
    connection: &str,
    operation: &str,
    mode: &str,
) {
    let workspace: Option<String> = query(&db.db,"SELECT mode FROM relay_connector_rules WHERE workspace=$1 AND connection=$2 AND operation=$3",vec![w.into(),connection.into(),operation.into()]).await.unwrap().map(|r| r.try_get("", "mode").unwrap());
    if workspace.as_deref() != Some("allow") {
        workspace_rule(db, admin, w, connection, operation, "allow").await;
    }
    db.control_agent_connection(
        admin,
        w,
        agent,
        &AgentConnection {
            connection: connection.into(),
            enabled: true,
        },
    )
    .await
    .unwrap();
    let revision: i64 = query(&db.db,"SELECT revision FROM relay_control_rules WHERE workspace=$1 AND agent=$2 AND connection=$3 AND operation=$4",vec![w.into(),agent.into(),connection.into(),operation.into()]).await.unwrap().map(|r| r.try_get("", "revision").unwrap()).unwrap_or(0);
    db.control_rule(
        admin,
        w,
        agent,
        &crate::identity::control::model::Rule {
            connection: connection.into(),
            operation: operation.into(),
            mode: mode.into(),
            revision,
        },
    )
    .await
    .unwrap();
}
