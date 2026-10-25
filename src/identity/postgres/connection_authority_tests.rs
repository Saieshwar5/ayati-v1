use super::{tests::*, *};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    control::{
        connections_model::{Creator, Delegation, SystemApproval, WorkspaceConnection},
        model::{ActionRequest, Decision, Rule},
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
        scope: "workspace".into(),
        revision: 0,
        settings: Settings::default(),
    }
}
async fn setup() -> Setup {
    let db = fixture().await;
    let owner = user(&db, "connections-owner").await;
    let member = user(&db, "connections-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "User").await;
    let creator = Creator {
        user: member.id.clone(),
        enabled: true,
        revision: 0,
    };
    db.control_creator(&owner, &w, &creator).await.unwrap();
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
    fn grant(&self, ceiling: &str, revision: i64) -> Delegation {
        Delegation {
            user: self.member.id.clone(),
            connection: self.account.id.clone(),
            operation: "order.update".into(),
            ceiling: ceiling.into(),
            revision,
            connection_revision: 1,
        }
    }
    fn rule(&self, mode: &str, revision: i64, grant_revision: i64) -> Rule {
        Rule {
            approver: None,
            approver_revision: None,
            approval_chat: None,
            connection: self.account.id.clone(),
            operation: "order.update".into(),
            mode: mode.into(),
            revision,
            chat: self.chat.clone(),
            delegation_revision: Some(grant_revision),
        }
    }
    async fn job(&self) -> crate::identity::chat::store::Job {
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
async fn creator_right_is_explicit_separate_from_tools_and_revocable() {
    let db = fixture().await;
    let owner = user(&db, "creator-owner").await;
    let member = user(&db, "creator-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "User").await;
    let a = agent();
    assert!(!db.can_create_workspace_agent(&member, &w).await.unwrap());
    assert!(db.save_agent_profile(&member, &w, &a).await.is_err());
    let mut grant = Creator {
        user: member.id.clone(),
        enabled: true,
        revision: 0,
    };
    assert!(db.control_creator(&member, &w, &grant).await.is_err());
    db.control_creator(&owner, &w, &grant).await.unwrap();
    db.control_creator(&owner, &w, &grant).await.unwrap(); // Exact retry.
    db.save_agent_profile(&member, &w, &a).await.unwrap();
    let snapshot = db.control_snapshot(&member, &w, &a.id).await.unwrap();
    assert!(snapshot["connections"].as_array().unwrap().is_empty());
    assert!(snapshot["delegations"].as_array().unwrap().is_empty());
    grant.enabled = false;
    assert!(db.control_creator(&owner, &w, &grant).await.is_err()); // Stale different payload.
    grant.revision = 1;
    db.control_creator(&owner, &w, &grant).await.unwrap();
    assert!(db.save_agent_profile(&member, &w, &agent()).await.is_err());
    let mut existing = a.clone();
    existing.revision = 1;
    existing.name = "Still maintained".into();
    db.save_agent_profile(&member, &w, &existing).await.unwrap();
    let mut private = agent();
    private.scope = "private".into();
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
async fn delegated_ask_ceiling_binds_exact_owner_and_revocation_fences_pending_work() {
    let s = setup().await;
    assert!(
        s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("allow", 0, 0))
            .await
            .is_err()
    );
    let grant = s.grant("ask", 0);
    assert!(
        s.db.control_delegate(&s.member, &s.w, &grant)
            .await
            .is_err()
    );
    s.db.control_delegate(&s.owner, &s.w, &grant).await.unwrap();
    s.db.control_delegate(&s.owner, &s.w, &grant).await.unwrap();
    assert!(
        s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("allow", 0, 1))
            .await
            .is_err()
    );
    assert!(
        s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("ask", 0, 0))
            .await
            .is_err()
    );
    s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("ask", 0, 1))
        .await
        .unwrap();
    let job = s.job().await;
    let pending = s.db.control_propose(&job, &s.request()).await.unwrap();
    assert_eq!(pending.action["state"], "pending");
    assert_eq!(pending.action["approver"], s.owner.id);
    let decision = Decision {
        id: uuid::Uuid::new_v4().to_string(),
        decision: "approve".into(),
    };
    let id = pending.action["id"].as_str().unwrap();
    assert!(
        s.db.control_decide(&s.member, &s.w, &s.agent.id, id, &decision)
            .await
            .is_err()
    );
    s.db.control_delegate(&s.owner, &s.w, &s.grant("block", 1))
        .await
        .unwrap();
    assert!(
        s.db.control_decide(&s.owner, &s.w, &s.agent.id, id, &decision)
            .await
            .is_err()
    );
    assert_eq!(
        s.db.control_snapshot(&s.owner, &s.w, &s.agent.id)
            .await
            .unwrap()["actions"][0]["state"],
        "invalidated"
    );
    assert!(
        !s.db
            .control_enabled(&s.w, &s.agent.id, &s.chat)
            .await
            .unwrap()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn delegated_rules_require_agent_ownership_and_approver_conversation_visibility() {
    let s = setup().await;
    s.db.control_delegate(&s.owner, &s.w, &s.grant("allow", 0))
        .await
        .unwrap();
    let (other_chat, other_agent) = super::executor_slot_tests::agent(&s.db, &s.owner, &s.w).await;
    let mut other = s.rule("allow", 0, 1);
    other.chat = other_chat;
    assert!(
        s.db.control_rule(&s.member, &s.w, &other_agent, &other)
            .await
            .is_err()
    );
    let private = chat(&s.db, &s.member, &s.w, vec![], vec![s.agent.id.clone()]).await;
    let mut rule = s.rule("ask", 0, 1);
    rule.chat = private;
    assert!(
        s.db.control_rule(&s.member, &s.w, &s.agent.id, &rule)
            .await
            .is_err()
    );
    rule = s.rule("allow", 0, 1);
    rule.operation = "unreviewed.operation".into();
    assert!(
        s.db.control_rule(&s.member, &s.w, &s.agent.id, &rule)
            .await
            .is_err()
    );
    let outsider = user(&s.db, "no-connection-access").await;
    join(&s.db, &s.owner, &s.w, &outsider, "User").await;
    s.db.chat_grant(&s.owner, &s.w, &outsider.id, &s.agent.id, true)
        .await
        .unwrap();
    let view =
        s.db.control_snapshot(&outsider, &s.w, &s.agent.id)
            .await
            .unwrap();
    assert!(view["connections"].as_array().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn delegation_revocation_after_claim_is_checked_again_before_dispatch() {
    let s = setup().await;
    s.db.control_delegate(&s.owner, &s.w, &s.grant("allow", 0))
        .await
        .unwrap();
    s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("allow", 0, 1))
        .await
        .unwrap();
    let job = s.job().await;
    let dispatch =
        s.db.control_propose(&job, &s.request())
            .await
            .unwrap()
            .dispatch
            .unwrap();
    s.db.control_ready(&s.w, &dispatch).await.unwrap();
    s.db.control_delegate(&s.owner, &s.w, &s.grant("block", 1))
        .await
        .unwrap();
    assert!(s.db.control_ready(&s.w, &dispatch).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn role_changes_and_rejoining_do_not_restore_old_agent_authority() {
    let s = setup().await;
    s.db.control_delegate(&s.owner, &s.w, &s.grant("allow", 0))
        .await
        .unwrap();
    s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("allow", 0, 1))
        .await
        .unwrap();
    s.db.change(&s.owner, &s.w, &s.member.id, Some("Owner"))
        .await
        .unwrap();
    assert!(
        !s.db
            .control_enabled(&s.w, &s.agent.id, &s.chat)
            .await
            .unwrap()
    );
    s.db.change(&s.owner, &s.w, &s.member.id, Some("User"))
        .await
        .unwrap();
    assert!(
        !s.db
            .can_create_workspace_agent(&s.member, &s.w)
            .await
            .unwrap()
    );
    s.db.control_delegate(&s.owner, &s.w, &s.grant("allow", 3))
        .await
        .unwrap();
    s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("allow", 4, 4))
        .await
        .unwrap();
    s.db.change(&s.owner, &s.w, &s.member.id, None)
        .await
        .unwrap();
    join(&s.db, &s.owner, &s.w, &s.member, "User").await;
    assert!(
        !s.db
            .control_enabled(&s.w, &s.agent.id, &s.chat)
            .await
            .unwrap()
    );
    assert!(
        !s.db
            .can_create_workspace_agent(&s.member, &s.w)
            .await
            .unwrap()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn re_reviewing_a_connection_blocks_delegations_as_well_as_agent_rules() {
    let s = setup().await;
    s.db.control_delegate(&s.owner, &s.w, &s.grant("allow", 0))
        .await
        .unwrap();
    s.db.control_rule(&s.member, &s.w, &s.agent.id, &s.rule("allow", 0, 1))
        .await
        .unwrap();
    let mut account = s.account;
    account.revision = 1;
    s.db.workspace_connection_save(
        &s.owner,
        &s.w,
        &account,
        Some((definition(), "reviewed-again")),
    )
    .await
    .unwrap();
    let view = s.db.workspace_connections(&s.owner, &s.w).await.unwrap();
    assert_eq!(view["delegations"][0]["ceiling"], "block");
    assert!(
        !s.db
            .control_enabled(&s.w, &s.agent.id, &s.chat)
            .await
            .unwrap()
    );
}
