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
    /// Ask first, decided by the agent's owner: `approver`, a member in the
    /// conversation. `owner` stays the workspace admin who asks for the work.
    async fn configure(&self) {
        self.give_to(&self.approver).await;
        self.ask_first().await;
    }
    /// Ask first on an agent still owned by the admin who asks for the work.
    async fn ask_first(&self) {
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
    /// Makes `person` the agent's one owner.
    async fn give_to(&self, person: &User) {
        let revision = self
            .db
            .managed_agent(&self.owner, &self.w, &self.agent, true)
            .await
            .unwrap()["revision"]
            .as_i64()
            .unwrap();
        self.db
            .agent_transfer(
                &self.owner,
                &self.w,
                &self.agent,
                &super::agent_audience::TransferAgent {
                    owner: person.id.clone(),
                    revision,
                },
            )
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
async fn the_agents_owner_decides_and_the_first_decision_wins() {
    let s = setup().await;
    s.configure().await;
    let (_, a) = s.propose().await;
    let id = a["id"].as_str().unwrap();
    let details = s.db.approval_details(&s.approver, &s.w, id).await.unwrap();
    assert_eq!(details["can_decide"], true);
    let names: Vec<_> = details["deciders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        std::slice::from_ref(&s.approver.id),
        "one owner decides"
    );
    // An admin who doesn't own the agent waits for its owner.
    let early =
        s.db.control_decide(&s.owner, &s.w, &s.agent, id, &decision("approve"))
            .await;
    let refusal = early.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(refusal.contains("owner decides first"), "{refusal}");
    // Someone in the conversation who doesn't own the agent cannot decide.
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
    // A repeated decision does not dispatch again, and cannot reverse it.
    assert!(
        s.db.control_decide(&s.approver, &s.w, &s.agent, id, &decision("approve"))
            .await
            .unwrap()
            .dispatch
            .is_none()
    );
    assert!(
        s.db.control_decide(&s.approver, &s.w, &s.agent, id, &decision("reject"))
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
    // The admin asked, so they can't approve it; the agent's owner does.
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

    // When the agent's owner asked for it themselves, another admin may decide
    // straight away.
    let s = setup().await;
    s.ask_first().await;
    s.needs_someone_else().await;
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

    // Otherwise an admin who isn't the owner waits for the backup delay.
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
    // The agent's owner decides here, in the conversation; the card names them.
    assert_eq!(cards[0]["approval"]["may_decide"], true);
    assert!(
        !cards[0]["approval"]["waiting_for"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(cards[0]["approval"]["backup_at"].as_i64().unwrap() > 0);
    assert!(
        !serde_json::to_string(&history)
            .unwrap()
            .contains("PRIVATE_EXACT_ADDRESS")
    );
    // Someone who joins later, and doesn't own the agent, sees the card but not the request.
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
    // In the conversation they see whom it waits for, without a decision.
    let theirs =
        s.db.chat_page(&s.other, &s.w, &s.source, None)
            .await
            .unwrap();
    let card = theirs["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["approval"]["id"] == id)
        .unwrap()
        .clone();
    assert_eq!(card["approval"]["may_decide"], false);
    assert_eq!(
        card["approval"]["waiting_for"],
        cards[0]["approval"]["waiting_for"]
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_owner_who_loses_access_cannot_dispatch_and_a_removed_owners_request_goes_to_admins() {
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
    // The agent's owner is removed with no new owner named. An agent acts
    // through its owner's rules, so with none nothing can be approved, not
    // even by an admin: what was waiting waits for the new owner.
    s.db.change(&s.owner, &s.w, &s.approver.id, None, None)
        .await
        .unwrap();
    let details = s.db.approval_details(&s.owner, &s.w, id).await.unwrap();
    assert_eq!(details["state"], "pending", "{details}");
    assert_eq!(details["can_decide"], false);
    assert!(details["deciders"].as_array().unwrap().is_empty());
    s.give_to(&s.owner).await;
    let details = s.db.approval_details(&s.owner, &s.w, id).await.unwrap();
    assert_eq!(details["can_decide"], true, "{details}");
    assert_eq!(details["deciders"][0]["id"], s.owner.id.as_str());
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
