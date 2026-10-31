use super::{tests::*, *};
use crate::identity::{
    chat::model::Job,
    control::{
        connections_model::{ConnectorRule, SystemApproval},
        model::{ActionRequest, Decision},
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
    connection: String,
}
async fn setup() -> Setup {
    let db = fixture().await;
    let owner = user(&db, "approval-owner").await;
    let approver = user(&db, "approval-person").await;
    let other = user(&db, "approval-observer").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &approver, "Member").await;
    join(&db, &owner, &w, &other, "Member").await;
    let (_, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let source = chat(
        &db,
        &owner,
        &w,
        vec![approver.id.clone()],
        vec![agent.clone()],
    )
    .await;
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
        connection,
    }
}
impl Setup {
    /// Ask first, decided by the agent's managers: its owner and `approver`.
    async fn configure(&self) {
        self.managers(&[&self.owner.id, &self.approver.id]).await;
        super::tool_test_support::grant(
            &self.db,
            &self.owner,
            &self.w,
            &self.agent,
            &self.connection,
            "order.update",
            "ask",
        )
        .await;
    }
    async fn managers(&self, people: &[&str]) {
        let mut profile = self
            .db
            .managed_agent(&self.owner, &self.w, &self.agent, true)
            .await
            .unwrap();
        profile["managers"] = json!(people);
        profile["people"] = json!(null);
        let input: crate::identity::agents::config::SaveAgent = serde_json::from_value(json!({
            "id":self.agent,"name":profile["name"],"role":profile["role"],"model":profile["model"],
            "audience":profile["audience"],"revision":profile["revision"],"settings":profile["settings"],
            "managers":people}))
        .unwrap();
        self.db
            .save_agent_profile(&self.owner, &self.w, &input)
            .await
            .unwrap();
    }
    async fn needs_someone_else(&self) {
        let revision: i64 = query(
            &self.db.db,
            "SELECT revision FROM relay_connector_rules WHERE workspace=$1",
            vec![self.w.clone().into()],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "revision")
        .unwrap();
        self.db
            .control_connector_rule(
                &self.owner,
                &self.w,
                &ConnectorRule {
                    connection: self.connection.clone(),
                    operation: "order.update".into(),
                    mode: "ask".into(),
                    needs_other: true,
                    revision,
                },
            )
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
async fn every_manager_may_decide_and_the_first_decision_wins() {
    let s = setup().await;
    s.configure().await;
    let (_, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    let details = s.db.approval_details(&s.approver, &s.w, id).await.unwrap();
    assert_eq!(details["can_decide"], true);
    let mut names: Vec<_> = details["deciders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    // Managers added in the same second have no meaningful order.
    names.sort();
    let mut expected = [s.owner.id.clone(), s.approver.id.clone()];
    expected.sort();
    assert_eq!(names, expected);
    // Someone in the conversation who doesn't manage the agent cannot decide.
    execute(
        &s.db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![
            s.w.clone().into(),
            s.source.clone().into(),
            s.other.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert!(
        s.db.control_decide(&s.other, &s.w, &s.agent, id, &decision("approve"))
            .await
            .is_err()
    );

    let approved =
        s.db.control_decide(&s.approver, &s.w, &s.agent, id, &decision("approve"))
            .await
            .unwrap();
    assert!(approved.dispatch.is_some());
    // The owner's later decision does not dispatch again, and cannot reverse it.
    assert!(
        s.db.control_decide(&s.owner, &s.w, &s.agent, id, &decision("approve"))
            .await
            .unwrap()
            .dispatch
            .is_none()
    );
    assert!(
        s.db.control_decide(&s.owner, &s.w, &s.agent, id, &decision("reject"))
            .await
            .is_err()
    );
    let decided: String = query(
        &s.db.db,
        "SELECT decided_by FROM relay_control_actions WHERE id=$1",
        vec![id.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "decided_by")
    .unwrap();
    assert_eq!(decided, s.approver.id);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn someone_else_must_approve_and_admins_step_in_after_the_backup_delay() {
    let s = setup().await;
    s.configure().await;
    s.needs_someone_else().await;
    let (_, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    // The owner asked, so only the other manager may approve.
    assert!(
        s.db.control_decide(&s.owner, &s.w, &s.agent, id, &decision("approve"))
            .await
            .is_err()
    );
    let details = s.db.approval_details(&s.owner, &s.w, id).await.unwrap();
    assert_eq!(details["needs_other"], true);
    assert_eq!(details["deciders"].as_array().unwrap().len(), 1);
    s.db.control_decide(&s.approver, &s.w, &s.agent, id, &decision("approve"))
        .await
        .unwrap();

    // With only the requester managing it, an admin may decide straight away.
    let s = setup().await;
    s.configure().await;
    s.needs_someone_else().await;
    s.managers(&[&s.owner.id]).await;
    let (_, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    s.db.change(&s.owner, &s.w, &s.other.id, Some("Admin"), None)
        .await
        .unwrap();
    execute(
        &s.db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![
            s.w.clone().into(),
            s.source.clone().into(),
            s.other.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        s.db.approval_details(&s.other, &s.w, id).await.unwrap()["can_decide"],
        true
    );

    // Otherwise an admin who isn't a manager waits for the backup delay.
    let s = setup().await;
    s.configure().await;
    let (_, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    s.db.change(&s.owner, &s.w, &s.other.id, Some("Admin"), None)
        .await
        .unwrap();
    execute(
        &s.db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![
            s.w.clone().into(),
            s.source.clone().into(),
            s.other.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert!(
        s.db.control_decide(&s.other, &s.w, &s.agent, id, &decision("approve"))
            .await
            .is_err()
    );
    assert!(
        s.db.approval_inbox(&s.other, &s.w).await.unwrap()["actions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    execute(
        &s.db.db,
        "UPDATE relay_control_actions SET created=created-4*3600-1 WHERE id=$1",
        vec![id.into()],
    )
    .await
    .unwrap();
    assert_eq!(
        s.db.approval_inbox(&s.other, &s.w).await.unwrap()["actions"][0]["id"],
        id
    );
    assert!(
        s.db.control_decide(&s.other, &s.w, &s.agent, id, &decision("approve"))
            .await
            .unwrap()
            .dispatch
            .is_some()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_request_card_is_a_reference_and_current_access_controls_its_details() {
    let s = setup().await;
    s.configure().await;
    let (job, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    s.db.control_propose(&job, &s.request()).await.unwrap();
    let history =
        s.db.chat_page(&s.approver, &s.w, &s.source, None)
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
    // A non-manager who joins later sees the card but not the request.
    execute(
        &s.db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![
            s.w.clone().into(),
            s.source.clone().into(),
            s.other.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    let details = s.db.approval_details(&s.other, &s.w, id).await.unwrap();
    assert_eq!(details["can_decide"], false);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_removed_manager_leaves_the_request_with_the_others_and_cannot_dispatch() {
    let s = setup().await;
    s.configure().await;
    let (_, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    let dispatch =
        s.db.control_decide(&s.approver, &s.w, &s.agent, id, &decision("approve"))
            .await
            .unwrap()
            .dispatch
            .unwrap();
    // Losing the conversation before dispatch fences the approved action.
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

    let s = setup().await;
    s.configure().await;
    let (_, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    s.db.change(&s.owner, &s.w, &s.approver.id, None, None)
        .await
        .unwrap();
    let details = s.db.approval_details(&s.owner, &s.w, id).await.unwrap();
    assert_eq!(details["state"], "pending");
    assert_eq!(details["can_decide"], true);
    assert_eq!(details["deciders"].as_array().unwrap().len(), 1);
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
