use super::{tests::*, *};
use crate::identity::{
    chat::store::Job,
    control::{
        approval_model::ApproverGrant,
        connections_model::SystemApproval,
        model::{ActionRequest, Decision, Rule},
    },
};
use serde_json::json;
struct Setup {
    db: PgStore,
    owner: User,
    approver: User,
    other: User,
    w: String,
    agent: String,
    source: String,
    target: String,
    connection: String,
}
async fn setup() -> Setup {
    let db = fixture().await;
    let owner = user(&db, "approval-owner").await;
    let approver = user(&db, "approval-person").await;
    let other = user(&db, "approval-observer").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &approver, "User").await;
    join(&db, &owner, &w, &other, "User").await;
    let (_, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let source = chat(
        &db,
        &owner,
        &w,
        vec![approver.id.clone()],
        vec![agent.clone()],
    )
    .await;
    let target = chat(&db, &owner, &w, vec![approver.id.clone()], vec![]).await;
    let connection = uuid::Uuid::new_v4().to_string();
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
    db.control_connect(&owner,&w,&agent,&connection,json!({"id":"orders","name":"Company orders","operations":[{"id":"order.update","title":"Update order","tool":"update_order","effect":"write","schema":{"type":"object","properties":{"order":{"type":"string"},"address":{"type":"string"}},"required":["order","address"],"additionalProperties":false}}]}),"reviewed").await.unwrap();
    Setup {
        db,
        owner,
        approver,
        other,
        w,
        agent,
        source,
        target,
        connection,
    }
}
impl Setup {
    fn grant(&self, enabled: bool, revision: i64) -> ApproverGrant {
        ApproverGrant {
            user: self.approver.id.clone(),
            connection: self.connection.clone(),
            operation: "order.update".into(),
            enabled,
            revision,
            connection_revision: 1,
        }
    }
    fn rule(&self) -> Rule {
        Rule {
            approver: Some(self.approver.id.clone()),
            approver_revision: Some(1),
            approval_chat: Some(self.target.clone()),
            connection: self.connection.clone(),
            operation: "order.update".into(),
            mode: "ask".into(),
            revision: 0,
            chat: self.source.clone(),
            delegation_revision: None,
        }
    }
    async fn configure(&self) {
        self.db
            .control_approver_grant(&self.owner, &self.w, &self.grant(true, 0))
            .await
            .unwrap();
        self.db
            .control_rule(&self.owner, &self.w, &self.agent, &self.rule())
            .await
            .unwrap();
    }
    async fn job(&self) -> Job {
        self.db
            .chat_send(
                &self.owner,
                &self.w,
                &self.source,
                &message("Change the address", Some(&self.agent)),
            )
            .await
            .unwrap();
        self.db.claim_job().await.unwrap().unwrap()
    }
    async fn propose(&self) -> (Job, Value) {
        let job = self.job().await;
        let p = self
            .db
            .control_propose(&job, &self.request())
            .await
            .unwrap();
        assert!(p.dispatch.is_none());
        (job, p.action)
    }
    fn request(&self) -> ActionRequest {
        ActionRequest {
            connection: self.connection.clone(),
            operation: "order.update".into(),
            arguments: json!({"order":"one","address":"PRIVATE_EXACT_ADDRESS"}),
        }
    }
}
fn decision(value: &str) -> Decision {
    Decision {
        id: uuid::Uuid::new_v4().to_string(),
        decision: value.into(),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn explicit_non_owner_authority_selects_one_approver_without_configuration_rights() {
    let s = setup().await;
    assert!(
        s.db.control_rule(&s.owner, &s.w, &s.agent, &s.rule())
            .await
            .is_err()
    );
    assert!(
        s.db.control_approver_grant(&s.approver, &s.w, &s.grant(true, 0))
            .await
            .is_err()
    );
    s.configure().await;
    assert!(
        !s.db
            .agent_access(&s.approver.id, &s.w, &s.agent)
            .await
            .unwrap()
    );
    assert!(
        s.db.chat_send(
            &s.approver,
            &s.w,
            &s.source,
            &message("Not authorized to start work", Some(&s.agent))
        )
        .await
        .is_err()
    );
    assert!(
        s.db.control_rule(&s.approver, &s.w, &s.agent, &s.rule())
            .await
            .is_err()
    );
    let (_, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    assert!(
        s.db.control_decide(&s.owner, &s.w, &s.agent, id, &decision("approve"))
            .await
            .is_err()
    );
    assert!(
        s.db.control_decide(&s.other, &s.w, &s.agent, id, &decision("approve"))
            .await
            .is_err()
    );
    assert_eq!(
        s.db.approval_details(&s.approver, &s.w, id).await.unwrap()["can_decide"],
        true
    );
    let approved =
        s.db.control_decide(&s.approver, &s.w, &s.agent, id, &decision("approve"))
            .await
            .unwrap();
    s.db.control_ready(&s.w, &approved.dispatch.unwrap())
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn delivery_is_one_saved_reference_and_current_source_access_controls_details() {
    let s = setup().await;
    s.configure().await;
    let (job, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    s.db.control_propose(&job, &s.request()).await.unwrap();
    let history =
        s.db.chat_page(&s.approver, &s.w, &s.target, None)
            .await
            .unwrap();
    let cards: Vec<_> = history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["approval"].is_object())
        .collect();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["approval"]["id"], id);
    assert_eq!(cards[0]["content"], "Approval request");
    assert!(
        !serde_json::to_string(&history)
            .unwrap()
            .contains("PRIVATE_EXACT_ADDRESS")
    );
    execute(
        &s.db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![
            s.w.clone().into(),
            s.target.clone().into(),
            s.other.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    let other =
        s.db.chat_page(&s.other, &s.w, &s.target, None)
            .await
            .unwrap();
    assert_eq!(other["messages"][0]["approval"]["restricted"], true);
    assert!(other["messages"][0]["approval"]["id"].is_null());
    assert!(s.db.approval_details(&s.other, &s.w, id).await.is_err());
    s.db.change(&s.owner, &s.w, &s.other.id, Some("Owner"))
        .await
        .unwrap();
    assert!(s.db.approval_details(&s.other, &s.w, id).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn broader_destination_is_rejected_and_later_audience_change_uses_restricted_inbox() {
    let s = setup().await;
    s.db.control_approver_grant(&s.owner, &s.w, &s.grant(true, 0))
        .await
        .unwrap();
    execute(
        &s.db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![
            s.w.clone().into(),
            s.target.clone().into(),
            s.other.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert!(
        s.db.control_rule(&s.owner, &s.w, &s.agent, &s.rule())
            .await
            .is_err()
    );
    execute(
        &s.db.db,
        "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
        vec![
            s.w.clone().into(),
            s.target.clone().into(),
            s.other.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    s.db.control_rule(&s.owner, &s.w, &s.agent, &s.rule())
        .await
        .unwrap();
    execute(
        &s.db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![
            s.w.clone().into(),
            s.target.clone().into(),
            s.other.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    let (_, a) = s.propose().await;
    assert!(
        s.db.chat_page(&s.approver, &s.w, &s.target, None)
            .await
            .unwrap()["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let inbox = s.db.approval_inbox(&s.approver, &s.w).await.unwrap();
    assert_eq!(inbox["actions"][0]["id"], a["id"]);
    assert_eq!(inbox["actions"][0]["delivery"]["status"], "inbox");
    assert!(
        s.db.approval_inbox(&s.other, &s.w).await.unwrap()["actions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn revocation_fences_pending_request_and_reauthorization_cannot_revive_it() {
    let s = setup().await;
    s.configure().await;
    let (_, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    s.db.control_approver_grant(&s.owner, &s.w, &s.grant(false, 1))
        .await
        .unwrap();
    assert!(
        s.db.control_decide(&s.approver, &s.w, &s.agent, id, &decision("approve"))
            .await
            .is_err()
    );
    s.db.control_approver_grant(&s.owner, &s.w, &s.grant(true, 2))
        .await
        .unwrap();
    assert!(
        s.db.control_decide(&s.approver, &s.w, &s.agent, id, &decision("approve"))
            .await
            .is_err()
    );
    assert_eq!(
        s.db.approval_details(&s.approver, &s.w, id).await.unwrap()["state"],
        "invalidated"
    );
    let history =
        s.db.chat_page(&s.approver, &s.w, &s.target, None)
            .await
            .unwrap();
    assert_eq!(history["messages"][0]["approval"]["state"], "invalidated");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn source_access_and_grantor_role_are_rechecked_after_approval_before_dispatch() {
    let s = setup().await;
    s.configure().await;
    let (_, a) = s.propose().await;
    let dispatch =
        s.db.control_decide(
            &s.approver,
            &s.w,
            &s.agent,
            a["id"].as_str().unwrap(),
            &decision("approve"),
        )
        .await
        .unwrap()
        .dispatch
        .unwrap();
    execute(
        &s.db.db,
        "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
        vec![
            s.w.clone().into(),
            s.source.clone().into(),
            s.approver.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert!(s.db.control_ready(&s.w, &dispatch).await.is_err());
    assert!(
        s.db.approval_details(&s.approver, &s.w, a["id"].as_str().unwrap())
            .await
            .is_err()
    );
    let s = setup().await;
    s.configure().await;
    let (_, a) = s.propose().await;
    s.db.change(&s.owner, &s.w, &s.other.id, Some("Owner"))
        .await
        .unwrap();
    s.db.change(&s.other, &s.w, &s.owner.id, Some("User"))
        .await
        .unwrap();
    assert!(
        s.db.control_decide(
            &s.approver,
            &s.w,
            &s.agent,
            a["id"].as_str().unwrap(),
            &decision("approve")
        )
        .await
        .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn request_changes_is_recorded_without_dispatch_and_opposite_decision_conflicts() {
    let s = setup().await;
    s.configure().await;
    let (_, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    let result =
        s.db.control_decide(
            &s.approver,
            &s.w,
            &s.agent,
            id,
            &decision("request_changes"),
        )
        .await
        .unwrap();
    assert!(result.dispatch.is_none());
    assert_eq!(result.action["state"], "rejected");
    assert_eq!(result.action["decision"], "request_changes");
    assert!(
        s.db.control_decide(
            &s.approver,
            &s.w,
            &s.agent,
            id,
            &decision("request_changes")
        )
        .await
        .unwrap()
        .dispatch
        .is_none()
    );
    assert!(
        s.db.control_decide(&s.approver, &s.w, &s.agent, id, &decision("approve"))
            .await
            .is_err()
    );
    assert_eq!(
        s.db.approval_details(&s.approver, &s.w, id).await.unwrap()["arguments"]["address"],
        "PRIVATE_EXACT_ADDRESS"
    );
}

#[path = "approval_configuration_tests.rs"]
mod configuration;
