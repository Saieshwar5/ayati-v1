use super::{tests::*, *};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    control::{
        connections_model::{ConnectorRule, SystemApproval, WorkspaceConnection},
        model::{ActionRequest, AgentConnection, Rule},
    },
};
use serde_json::json;

struct Setup {
    db: PgStore,
    owner: User,
    member: User,
    w: String,
    agent: SaveAgent,
    chat: String,
    account: WorkspaceConnection,
}
fn agent() -> SaveAgent {
    SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Delegated agent".into(),
        role: "Orders".into(),
        model: "fixture".into(),
        audience: "selected".into(),
        people: None,
        managers: None,
        revision: 0,
        settings: Settings::default(),
    }
}
async fn setup() -> Setup {
    let db = fixture().await;
    let owner = user(&db, "connections-owner").await;
    let member = user(&db, "connections-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let agent = agent();
    db.save_agent_profile(&member, &w, &agent).await.unwrap();
    let chat = chat(
        &db,
        &member,
        &w,
        vec![owner.id.clone()],
        vec![agent.id.clone()],
    )
    .await;
    let account = WorkspaceConnection {
        id: uuid::Uuid::new_v4().to_string(),
        server: "orders".into(),
        enabled: true,
        revision: 0,
    };
    db.control_system(
        &owner,
        &w,
        &SystemApproval {
            server: "orders".into(),
            approved: true,
            revision: 0,
        },
    )
    .await
    .unwrap();
    db.workspace_connection_save(&owner, &w, &account, Some((definition(), "reviewed")))
        .await
        .unwrap();
    Setup {
        db,
        owner,
        member,
        w,
        agent,
        chat,
        account,
    }
}
fn definition() -> Value {
    json!({"id":"orders","name":"Orders", "operations":[{"id":"order.update","tool":"update_order","title":"Update order","effect":"write","schema":{"type":"object","properties":{"order":{"type":"string"}},"required":["order"],"additionalProperties":false}}]})
}
impl Setup {
    fn workspace(&self, mode: &str, revision: i64) -> ConnectorRule {
        ConnectorRule {
            connection: self.account.id.clone(),
            operation: "order.update".into(),
            mode: mode.into(),
            needs_other: false,
            revision,
        }
    }
    fn switch(&self, enabled: bool) -> AgentConnection {
        AgentConnection {
            connection: self.account.id.clone(),
            enabled,
        }
    }
    fn rule(&self, mode: &str, revision: i64) -> Rule {
        Rule {
            connection: self.account.id.clone(),
            operation: "order.update".into(),
            mode: mode.into(),
            revision,
        }
    }
    async fn job(&self) -> crate::identity::chat::model::Job {
        self.db
            .chat_send(
                &self.member,
                &self.w,
                &self.chat,
                &message("Update order", Some(&self.agent.id)),
            )
            .await
            .unwrap();
        self.db.claim_job().await.unwrap().unwrap()
    }
    fn request(&self) -> ActionRequest {
        ActionRequest {
            connection: self.account.id.clone(),
            operation: "order.update".into(),
            arguments: json!({"order":"one"}),
        }
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn every_member_may_create_agents_without_tools() {
    let db = fixture().await;
    let owner = user(&db, "creator-owner").await;
    let member = user(&db, "creator-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let a = agent();
    db.save_agent_profile(&member, &w, &a).await.unwrap();
    let snapshot = db.control_snapshot(&member, &w, &a.id).await.unwrap();
    assert!(snapshot["connections"].as_array().unwrap().is_empty());
    assert!(!db.control_enabled(&w, &a.id).await.unwrap());
    let mut private = agent();
    private.audience = "only_me".into();
    db.save_agent_profile(&member, &w, &private).await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn workspace_account_mutations_require_owner_approval_and_current_revisions() {
    let s = setup().await;
    assert!(s.db.workspace_connections(&s.member, &s.w).await.is_err());
    assert!(
        s.db.workspace_connection_save(
            &s.member,
            &s.w,
            &s.account,
            Some((definition(), "reviewed"))
        )
        .await
        .is_err()
    );
    let revoke = SystemApproval {
        server: "orders".into(),
        approved: false,
        revision: 1,
    };
    assert!(s.db.control_system(&s.member, &s.w, &revoke).await.is_err());
    let other = workspace(&s.db, &s.owner).await;
    assert!(
        s.db.workspace_connection_save(
            &s.owner,
            &other,
            &s.account,
            Some((definition(), "reviewed"))
        )
        .await
        .is_err()
    );
    s.db.workspace_connection_save(&s.owner, &s.w, &s.account, Some((definition(), "reviewed")))
        .await
        .unwrap();
    let mut changed = s.account;
    changed.enabled = false;
    assert!(
        s.db.workspace_connection_save(&s.owner, &s.w, &changed, None)
            .await
            .is_err()
    );
    s.db.control_system(&s.owner, &s.w, &revoke).await.unwrap();
    let snapshot = s.db.workspace_connections(&s.owner, &s.w).await.unwrap();
    assert_eq!(snapshot["connections"][0]["enabled"], false);
    changed.enabled = true;
    changed.revision = 2;
    assert!(
        s.db.workspace_connection_save(&s.owner, &s.w, &changed, Some((definition(), "reviewed")))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn new_connectors_start_safe_and_agent_rules_can_only_be_stricter() {
    let s = setup().await;
    let tools =
        s.db.control_snapshot(&s.member, &s.w, &s.agent.id)
            .await
            .unwrap();
    let op = &tools["connections"][0]["operations"][0];
    assert_eq!(tools["connections"][0]["on"], false);
    assert_eq!(
        (op["workspace_mode"].as_str(), op["mode"].as_str()),
        (Some("ask"), Some("ask"))
    );
    assert!(
        s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("block", 0))
            .await
            .is_err(),
        "switch on first"
    );

    // The agent's owner (a member) switches it on; the write asks first by default.
    s.db.control_agent_connection(&s.member, &s.w, &s.agent.id, &s.switch(true))
        .await
        .unwrap();
    assert!(s.db.control_enabled(&s.w, &s.agent.id).await.unwrap());
    let refused =
        s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("allow", 0))
            .await
            .unwrap_err();
    assert!(refused.to_string().contains("Ask first"), "{refused}");
    assert!(
        s.db.control_connector_rule(&s.member, &s.w, &s.workspace("allow", 0))
            .await
            .is_err(),
        "admins only"
    );
    let pending =
        s.db.control_propose(&s.job().await, &s.request())
            .await
            .unwrap();
    assert_eq!(pending.action["state"], "pending");

    // Stricter is fine, and fences the waiting request.
    s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("block", 0))
        .await
        .unwrap();
    let state: String = query(
        &s.db.db,
        "SELECT state FROM relay_control_actions WHERE id=$1",
        vec![pending.action["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "state")
    .unwrap();
    assert_eq!(state, "invalidated");
    assert!(!s.db.control_enabled(&s.w, &s.agent.id).await.unwrap());

    // The admin's rule is the loosest: Allow there still asks first under the agent's own Ask.
    s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("ask", 1))
        .await
        .unwrap();
    s.db.control_connector_rule(&s.owner, &s.w, &s.workspace("allow", 0))
        .await
        .unwrap();
    let tools =
        s.db.control_snapshot(&s.member, &s.w, &s.agent.id)
            .await
            .unwrap();
    let op = &tools["connections"][0]["operations"][0];
    assert_eq!(
        (
            op["workspace_mode"].as_str(),
            op["agent_mode"].as_str(),
            op["mode"].as_str()
        ),
        (Some("allow"), Some("ask"), Some("ask"))
    );
    let revision = op["revision"].as_i64().unwrap();
    s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("allow", revision))
        .await
        .unwrap();
    let job = s.job().await;
    assert!(
        s.db.control_propose(&job, &s.request())
            .await
            .unwrap()
            .dispatch
            .is_some()
    );

    // Blocking at the workspace stops every agent, whatever its own rule.
    let current: i64 = query(
        &s.db.db,
        "SELECT revision FROM relay_connector_rules WHERE workspace=$1",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "revision")
    .unwrap();
    s.db.control_connector_rule(&s.owner, &s.w, &s.workspace("block", current))
        .await
        .unwrap();
    // An exact retry after a lost response is accepted.
    s.db.control_connector_rule(&s.owner, &s.w, &s.workspace("block", current))
        .await
        .unwrap();
    assert!(
        s.db.control_connector_rule(&s.owner, &s.w, &s.workspace("ask", current))
            .await
            .is_err(),
        "stale"
    );
    assert!(!s.db.control_enabled(&s.w, &s.agent.id).await.unwrap());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn switching_off_and_re_reviewing_a_connection_remove_every_old_permission() {
    let s = setup().await;
    super::tool_test_support::grant(
        &s.db,
        &s.owner,
        &s.w,
        &s.agent.id,
        &s.account.id,
        "order.update",
        "allow",
    )
    .await;
    assert!(s.db.control_enabled(&s.w, &s.agent.id).await.unwrap());
    assert!(
        s.db.control_agent_connection(&s.owner, &s.w, "missing", &s.switch(true))
            .await
            .is_err()
    );
    s.db.control_agent_connection(&s.member, &s.w, &s.agent.id, &s.switch(false))
        .await
        .unwrap();
    assert!(!s.db.control_enabled(&s.w, &s.agent.id).await.unwrap());
    assert_eq!(
        s.db.control_propose(&s.job().await, &s.request())
            .await
            .unwrap()
            .action["state"],
        "denied"
    );

    super::tool_test_support::grant(
        &s.db,
        &s.owner,
        &s.w,
        &s.agent.id,
        &s.account.id,
        "order.update",
        "allow",
    )
    .await;
    let mut changed = definition();
    changed["operations"][0]["title"] = json!("Update an order's address");
    let mut account = s.account.clone();
    account.revision = 1;
    s.db.workspace_connection_save(&s.owner, &s.w, &account, Some((changed, "re-reviewed")))
        .await
        .unwrap();
    // New meanings start over: no switch, no agent rules, safe workspace defaults.
    for table in [
        "relay_agent_connections",
        "relay_control_rules",
        "relay_connector_rules",
    ] {
        let n: i64 = query(
            &s.db.db,
            &format!("SELECT count(*) AS n FROM {table} WHERE workspace=$1"),
            vec![s.w.clone().into()],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "n")
        .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    assert!(!s.db.control_enabled(&s.w, &s.agent.id).await.unwrap());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn role_changes_keep_pending_decisions_and_a_removed_owner_hands_tools_on() {
    let s = setup().await;
    super::tool_test_support::grant(
        &s.db,
        &s.owner,
        &s.w,
        &s.agent.id,
        &s.account.id,
        "order.update",
        "ask",
    )
    .await;
    let pending =
        s.db.control_propose(&s.job().await, &s.request())
            .await
            .unwrap();
    assert_eq!(pending.action["approver"], s.member.id);
    // Whoever may decide is checked when they decide, so a role change keeps it waiting.
    s.db.change(&s.owner, &s.w, &s.member.id, Some("Admin"), None)
        .await
        .unwrap();
    let state: String = query(
        &s.db.db,
        "SELECT state FROM relay_control_actions WHERE id=$1",
        vec![pending.action["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "state")
    .unwrap();
    assert_eq!(state, "pending");
    // The agent and its tools move to the admin; the switch stays on.
    s.db.change(&s.owner, &s.w, &s.member.id, None, None)
        .await
        .unwrap();
    assert!(s.db.control_enabled(&s.w, &s.agent.id).await.unwrap());
}
