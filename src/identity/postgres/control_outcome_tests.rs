//! Settling unconfirmed writes (schema 107): who may answer, repeats and conflicts,
//! the write hold, and the waiting task continuing with the answer.
use super::super::super::tests::{chat, fixture, join, message, user, workspace};
use super::super::super::*;
use crate::identity::{
    chat::model::Job,
    control::{
        connections_model::{ConnectorRule, SystemApproval},
        model::{ActionRequest, Decision},
        outcomes::{AnswerConflict, Settle},
    },
};
use serde_json::json;

/// A workspace whose agent writes orders, with the people who may or may not answer.
pub(super) struct Held {
    pub db: PgStore,
    pub owner: User,
    pub manager: User,
    pub admin: User,
    pub member: User,
    pub w: String,
    pub agent: String,
    pub connection: String,
    pub job: Job,
}

fn definition() -> Value {
    json!({"id":"orders","name":"Orders","operations":[{"id":"order.update","title":"Update order","tool":"update_order","effect":"write","schema":{"type":"object","properties":{"order":{"type":"string"},"address":{"type":"string"}},"required":["order","address"],"additionalProperties":false}}]})
}

/// `mode` is the action's rule; `needs_other` asks for someone other than the requester.
pub(super) async fn setup(mode: &str, needs_other: bool) -> Held {
    let db = fixture().await;
    let owner = user(&db, "outcome-owner").await;
    let w = workspace(&db, &owner).await;
    let mut people = vec![];
    for name in ["outcome-manager", "outcome-admin", "outcome-member"] {
        let person = user(&db, name).await;
        join(&db, &owner, &w, &person, "Member").await;
        people.push(person);
    }
    execute(
        &db.db,
        "UPDATE relay_members SET role='Admin' WHERE workspace=$1 AND user_id=$2",
        vec![w.clone().into(), people[1].id.clone().into()],
    )
    .await
    .unwrap();
    let (_, agent) = super::super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let ids = people.iter().map(|p| p.id.clone()).collect();
    let room = chat(&db, &owner, &w, ids, vec![agent.clone()]).await;
    execute(&db.db, "INSERT INTO relay_agent_managers(workspace,agent,user_id,added_by,at) VALUES($1,$2,$3,$4,1)",
        vec![w.clone().into(), agent.clone().into(), people[0].id.clone().into(), owner.id.clone().into()]).await.unwrap();
    let connection = uuid::Uuid::new_v4().to_string();
    let approval = SystemApproval {
        server: "orders".into(),
        approved: true,
        revision: 0,
    };
    db.control_system(&owner, &w, &approval).await.unwrap();
    db.control_connect(&owner, &w, &agent, &connection, definition(), "reviewed")
        .await
        .unwrap();
    super::super::super::tool_test_support::grant(
        &db,
        &owner,
        &w,
        &agent,
        &connection,
        "order.update",
        mode,
    )
    .await;
    if needs_other {
        let rule = ConnectorRule {
            connection: connection.clone(),
            operation: "order.update".into(),
            mode: mode.into(),
            needs_other: true,
            revision: revision(&db, &w).await,
        };
        db.control_connector_rule(&owner, &w, &rule).await.unwrap();
    }
    let ask = message("Update the sample order", Some(&agent));
    db.chat_send(&owner, &w, &room, &ask).await.unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let (member, admin) = (people.pop().unwrap(), people.pop().unwrap());
    let manager = people.pop().unwrap();
    Held {
        db,
        owner,
        manager,
        admin,
        member,
        w,
        agent,
        connection,
        job,
    }
}

async fn revision(db: &PgStore, w: &str) -> i64 {
    query(
        &db.db,
        "SELECT revision FROM relay_connector_rules WHERE workspace=$1",
        vec![w.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "revision")
    .unwrap()
}

pub(super) fn request(connection: &str, order: &str) -> ActionRequest {
    ActionRequest {
        connection: connection.into(),
        operation: "order.update".into(),
        arguments: json!({"order":order,"address":"Reviewed address"}),
    }
}

pub(super) fn answer(happened: bool, note: &str) -> Settle {
    Settle {
        happened,
        note: note.into(),
    }
}

impl Held {
    /// Sends `order` and loses the provider's answer. An "Ask first" action is
    /// approved by `approver` after the task has stopped to wait for it.
    pub(super) async fn unconfirmed(&self, order: &str, approver: &User) -> String {
        let proposed = self
            .db
            .control_propose(&self.job, &request(&self.connection, order))
            .await
            .unwrap();
        let id = proposed.action["id"].as_str().unwrap().to_owned();
        let dispatch = match proposed.dispatch {
            Some(dispatch) => dispatch,
            None => {
                self.db
                    .chat_finish(&self.job, "Waiting for approval", "complete")
                    .await
                    .unwrap();
                let decision = Decision {
                    id: uuid::Uuid::new_v4().to_string(),
                    decision: "approve".into(),
                };
                let decided = self
                    .db
                    .control_decide(approver, &self.w, &self.agent, &id, &decision)
                    .await
                    .unwrap();
                let dispatch = decided.dispatch.unwrap();
                self.db.control_ready(&self.w, &dispatch).await.unwrap();
                dispatch
            }
        };
        let lost = json!({"message":"The external outcome could not be confirmed."});
        let held = self
            .db
            .control_finish(&self.w, &dispatch, lost, false)
            .await
            .unwrap();
        assert_eq!(held["state"], "uncertain");
        id
    }
    pub(super) async fn job_state(&self) -> String {
        query(
            &self.db.db,
            "SELECT state FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2",
            vec![self.w.clone().into(), self.job.reply.clone().into()],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "state")
        .unwrap()
    }
}

fn is_held(result: anyhow::Result<impl Sized>) -> bool {
    result.is_err_and(|e| e.to_string().contains("needs reconciliation"))
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_manager_says_it_happened_and_the_accounts_next_write_goes_ahead() {
    let s = setup("allow", false).await;
    let id = s.unconfirmed("one", &s.owner).await;
    assert!(is_held(
        s.db.control_propose(&s.job, &request(&s.connection, "two"))
            .await
    ));
    let details = s.db.approval_details(&s.manager, &s.w, &id).await.unwrap();
    assert_eq!(details["state"], "uncertain");
    assert_eq!(details["settle"], json!({"can_settle":true,"checks":[]}));

    let settled =
        s.db.control_settle(&s.manager, &s.w, &id, &answer(true, " In Sent at 10:02 "))
            .await
            .unwrap();
    assert_eq!(settled["state"], "completed");
    assert_eq!(settled["settled"]["by"], s.manager.id.as_str());
    assert_eq!(
        settled["settled"]["by_name"],
        "outcome-manager@example.test"
    );
    assert_eq!(settled["settled"]["happened"], true);
    assert_eq!(settled["settled"]["note"], "In Sent at 10:02");
    assert!(
        settled["result"]["message"]
            .as_str()
            .unwrap()
            .contains("happened")
    );
    let next =
        s.db.control_propose(&s.job, &request(&s.connection, "two"))
            .await
            .unwrap();
    assert!(next.dispatch.is_some(), "the hold is released");

    let details = s.db.approval_details(&s.owner, &s.w, &id).await.unwrap();
    assert!(details.get("settle").is_none());
    assert_eq!(details["settled"]["happened"], true);
    let audited = query(&s.db.db, "SELECT actor,summary FROM relay_audit WHERE workspace=$1 AND action='control_action_settled' AND target=$2",
        vec![s.w.clone().into(), id.clone().into()]).await.unwrap().unwrap();
    assert_eq!(
        audited.try_get::<String>("", "actor").unwrap(),
        s.manager.id
    );
    assert_eq!(
        audited.try_get::<String>("", "summary").unwrap(),
        "said this happened: “Update order”"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_admin_may_answer_a_repeat_returns_the_answer_and_the_opposite_conflicts() {
    let s = setup("allow", false).await;
    let id = s.unconfirmed("one", &s.owner).await;
    let first =
        s.db.control_settle(&s.admin, &s.w, &id, &answer(false, ""))
            .await
            .unwrap();
    assert_eq!(first["state"], "not_sent");
    assert_eq!(first["settled"]["happened"], false);
    assert_eq!(first["settled"]["note"], Value::Null);
    for person in [&s.admin, &s.owner] {
        let again =
            s.db.control_settle(person, &s.w, &id, &answer(false, "Other words"))
                .await
                .unwrap();
        assert_eq!(again["settled"], first["settled"]);
    }
    let Err(conflict) =
        s.db.control_settle(&s.owner, &s.w, &id, &answer(true, ""))
            .await
    else {
        panic!("the opposite answer was saved");
    };
    assert!(conflict.is::<AnswerConflict>());
    let status = crate::identity::Error::from(conflict).1;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);

    // Only unconfirmed actions can be answered: a confirmed write cannot, and a
    // dispatch still running is answered only after the 60-second rule.
    let running =
        s.db.control_propose(&s.job, &request(&s.connection, "two"))
            .await
            .unwrap();
    let running_id = running.action["id"].as_str().unwrap().to_owned();
    assert!(
        s.db.control_settle(&s.owner, &s.w, &running_id, &answer(true, ""))
            .await
            .is_err()
    );
    execute(
        &s.db.db,
        "UPDATE relay_control_actions SET updated=updated-61 WHERE id=$1",
        vec![running_id.clone().into()],
    )
    .await
    .unwrap();
    let stale =
        s.db.control_settle(&s.owner, &s.w, &running_id, &answer(true, ""))
            .await
            .unwrap();
    assert_eq!(stale["state"], "completed");
    let dispatch = running.dispatch.unwrap();
    let late = s.db.control_finish(&s.w, &dispatch, json!({}), true).await;
    assert!(late.is_err(), "a late provider answer can't overwrite it");
    let confirmed =
        s.db.control_propose(&s.job, &request(&s.connection, "three"))
            .await
            .unwrap();
    s.db.control_finish(&s.w, &confirmed.dispatch.unwrap(), json!({}), true)
        .await
        .unwrap();
    let confirmed_id = confirmed.action["id"].as_str().unwrap();
    assert!(
        s.db.control_settle(&s.owner, &s.w, confirmed_id, &answer(false, ""))
            .await
            .is_err()
    );
    assert!(
        s.db.control_settle(&s.owner, &s.w, &id, &answer(false, &"n".repeat(501)))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_requester_cannot_answer_when_someone_else_approved_and_members_cannot_answer() {
    let s = setup("ask", true).await;
    let id = s.unconfirmed("one", &s.manager).await;
    // The owner asked: although an admin and a manager, they needed someone else.
    let refused =
        s.db.control_settle(&s.owner, &s.w, &id, &answer(true, ""))
            .await;
    assert!(
        refused
            .unwrap_err()
            .to_string()
            .contains("Someone other than")
    );
    // In the conversation but not managing the agent.
    assert!(
        s.db.control_settle(&s.member, &s.w, &id, &answer(true, ""))
            .await
            .is_err()
    );
    let details = s.db.approval_details(&s.member, &s.w, &id).await.unwrap();
    assert_eq!(details["settle"]["can_settle"], false);
    let details = s.db.approval_details(&s.owner, &s.w, &id).await.unwrap();
    assert_eq!(details["settle"]["can_settle"], false);
    let settled =
        s.db.control_settle(&s.admin, &s.w, &id, &answer(true, ""))
            .await
            .unwrap();
    assert_eq!(settled["state"], "completed");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn reconnecting_the_account_keeps_its_writes_held_until_someone_answers() {
    let s = setup("allow", false).await;
    let id = s.unconfirmed("one", &s.owner).await;
    s.db.chat_finish(&s.job, "Could not confirm the update", "complete")
        .await
        .unwrap();
    // An admin reconnects the account: same connection, new review, rules start over.
    s.db.control_connect(
        &s.owner,
        &s.w,
        &s.agent,
        &s.connection,
        definition(),
        "reconnected",
    )
    .await
    .unwrap();
    super::super::super::tool_test_support::grant(
        &s.db,
        &s.owner,
        &s.w,
        &s.agent,
        &s.connection,
        "order.update",
        "allow",
    )
    .await;
    let room = s.job.chat.clone();
    let ask = message("Update another order", Some(&s.agent));
    s.db.chat_send(&s.owner, &s.w, &room, &ask).await.unwrap();
    let next = s.db.claim_job().await.unwrap().unwrap();
    assert!(is_held(
        s.db.control_propose(&next, &request(&s.connection, "two"))
            .await
    ));
    s.db.control_settle(&s.manager, &s.w, &id, &answer(false, "Not in Sent"))
        .await
        .unwrap();
    let after =
        s.db.control_propose(&next, &request(&s.connection, "two"))
            .await
            .unwrap();
    assert!(after.dispatch.is_some());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_waiting_task_continues_and_is_told_the_write_was_not_sent() {
    let s = setup("ask", false).await;
    let id = s.unconfirmed("one", &s.owner).await;
    assert_eq!(s.job_state().await, "waiting");
    assert!(s.db.claim_job().await.unwrap().is_none());
    s.db.control_settle(&s.manager, &s.w, &id, &answer(false, "Nothing in Sent"))
        .await
        .unwrap();
    assert_eq!(s.job_state().await, "queued");
    let resumed = s.db.claim_job().await.unwrap().unwrap();
    assert_eq!(resumed.reply, s.job.reply);
    let saved = s.db.task_continuation(&resumed).await.unwrap().unwrap();
    let data: Value = serde_json::from_str(&saved).unwrap();
    let action = &data["actions"][0];
    assert_eq!(action["state"], "not_sent");
    let told = action["result"]["message"].as_str().unwrap();
    assert!(
        told.contains("was not sent") && told.contains("only if someone asks"),
        "{told}"
    );
    assert_eq!(action["result"]["note"], "Nothing in Sent");
    // The same exact request returns its receipt instead of sending it again.
    let repeated =
        s.db.control_propose(&resumed, &request(&s.connection, "one"))
            .await
            .unwrap();
    assert_eq!(repeated.action["id"], id.as_str());
    assert!(repeated.dispatch.is_none());
    let other =
        s.db.control_propose(&resumed, &request(&s.connection, "two"))
            .await
            .unwrap();
    assert_eq!(other.action["state"], "pending");
}
