use super::{tests::*, *};
use crate::{
    identity::{
        chat::model::Job,
        control::model::{ActionRequest, Rule},
        home_panels::{
            maintenance::{Agreement, Refresh, Update},
            model::Review,
        },
    },
    panels::PanelDraft,
};
use serde_json::json;

pub(super) async fn setup() -> (PgStore, User, String, Job, String, String, PanelDraft) {
    let (mut db, owner, w, write, job) = super::control_tests::setup("allow").await;
    let read = uuid::Uuid::new_v4().to_string();
    db.control_system(
        &owner,
        &w,
        &crate::identity::control::connections_model::SystemApproval {
            server: "order-reads".into(),
            approved: true,
            revision: 0,
        },
    )
    .await
    .unwrap();
    db.control_connect(&owner,&w,&job.agent,&read,json!({"id":"order-reads","name":"Order reads","operations":[{"id":"order.read","title":"Read order","tool":"read_order","effect":"read","schema":{"type":"object","properties":{"order":{"type":"string"}},"required":["order"],"additionalProperties":false}}]}),"reviewed-read").await.unwrap();
    db.control_rule(
        &owner,
        &w,
        &job.agent,
        &Rule {
            connection: read.clone(),
            operation: "order.read".into(),
            mode: "allow".into(),
            revision: 0,
            chat: job.chat.clone(),
            approver: None,
            approver_revision: None,
            approval_chat: None,
            delegation_revision: None,
        },
    )
    .await
    .unwrap();
    observed(&db, &job, &read).await;
    let panel:PanelDraft=serde_json::from_value(json!({"id":uuid::Uuid::new_v4().to_string(),"expected_revision":0,"title":"Personal order","purpose":"Track this reviewed order","template":"metric","sections":[{"kind":"metric","label":"Order count","value":"1"}],"sources":[]})).unwrap();
    db.save_personal_panel(&job, &panel).await.unwrap();
    db.review_home_panel(
        &owner,
        &w,
        &panel.id,
        &Review {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
        },
    )
    .await
    .unwrap();
    db.chat_finish(&job, "Prepared panel", "complete")
        .await
        .unwrap();
    db.pooled = true;
    (db, owner, w, job, read, write, panel)
}
pub(super) fn agreement(revision: i64, interval: Option<i64>) -> Agreement {
    Agreement {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        panel_revision: 1,
        enabled: true,
        instructions: "Read the reviewed order and update its count".into(),
        interval_seconds: interval,
        stale_seconds: 3600,
    }
}
pub(super) fn update(revision: i64) -> Update {
    Update {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
    }
}
pub(super) fn refresh(value: &str) -> Refresh {
    serde_json::from_value(
        json!({"sections":[{"kind":"metric","label":"Order count","value":value}]}),
    )
    .unwrap()
}
pub(super) async fn observed(db: &PgStore, job: &Job, connection: &str) {
    let action = db
        .control_propose(
            job,
            &ActionRequest {
                connection: connection.into(),
                operation: "order.read".into(),
                arguments: json!({"order":"one"}),
            },
        )
        .await
        .unwrap();
    let dispatch = action.dispatch.unwrap();
    db.control_ready(&job.workspace, &dispatch).await.unwrap();
    db.control_finish(&job.workspace, &dispatch, json!({"count":2}), true)
        .await
        .unwrap();
}
pub(super) async fn claim(db: &PgStore, w: &str, reply: &str) -> Job {
    // Storage fixtures install an exact pooled claim. The connected journey
    // separately exercises real executor admission, leases and the bridge.
    let claim = uuid::Uuid::new_v4().to_string();
    let row=query(&db.db,"UPDATE relay_chat_jobs SET state='running',claim=$3,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE workspace=$1 AND reply=$2 AND state='queued' RETURNING payload",vec![w.into(),reply.into(),claim.clone().into()]).await.unwrap().unwrap();
    let mut job: Job = serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap();
    job.claim = Some(claim);
    job
}
pub(super) async fn panel(db: &PgStore, owner: &User, w: &str) -> Value {
    db.home_panels(owner, w).await.unwrap()["panels"][0].clone()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn home_maintenance_exact_reads_automatic_publication_and_personal_authority() {
    let (db, owner, w, original, read, write, p) = setup().await;
    let input = agreement(0, None);
    let saved = db
        .save_home_agreement(&owner, &w, &p.id, &input)
        .await
        .unwrap();
    assert_eq!(
        db.save_home_agreement(&owner, &w, &p.id, &input)
            .await
            .unwrap(),
        saved
    );
    let other = user(&db, "other-home-owner").await;
    join(&db, &owner, &w, &other, "User").await;
    db.change(&owner, &w, &other.id, Some("Owner"))
        .await
        .unwrap();
    assert!(
        db.save_home_agreement(&other, &w, &p.id, &agreement(1, None))
            .await
            .is_err()
    );
    let foreign = workspace(&db, &owner).await;
    assert!(
        db.update_home_panel(&owner, &foreign, &p.id, &update(1))
            .await
            .is_err()
    );
    assert!(
        db.update_home_panel(&other, &w, &p.id, &update(1))
            .await
            .is_err()
    );
    assert!(
        db.update_home_panel(&owner, &w, &p.id, &update(2))
            .await
            .is_err()
    );
    let request = update(1);
    let second_request = update(1);
    let (one, two) = tokio::join!(
        db.update_home_panel(&owner, &w, &p.id, &request),
        db.update_home_panel(&owner, &w, &p.id, &second_request)
    );
    let reply = one.unwrap()["reply"].as_str().unwrap().to_string();
    assert_eq!(two.unwrap()["reply"], reply);
    let job = claim(&db, &w, &reply).await;
    assert!(db.refresh_home_panel(&job, &refresh("2")).await.is_err());
    assert!(
        db.control_propose(&job, &super::control_tests::request(&write, "one"))
            .await
            .is_err()
    );
    assert!(
        db.control_propose(
            &job,
            &ActionRequest {
                connection: read.clone(),
                operation: "order.read".into(),
                arguments: json!({"order":"someone-else"})
            }
        )
        .await
        .is_err()
    );
    assert!(
        crate::identity::control::broker::invoke(
            &db,
            &job,
            "save_home_panel",
            serde_json::to_value(&p).unwrap()
        )
        .await
        .is_err()
    );
    assert!(
        crate::identity::control::broker::invoke(&db, &job, "request_handoff", json!({}))
            .await
            .is_err()
    );
    observed(&db, &job, &read).await;
    let fresh = db.refresh_home_panel(&job, &refresh("2")).await.unwrap();
    assert_eq!(
        db.refresh_home_panel(&job, &refresh("2")).await.unwrap(),
        fresh
    );
    assert!(db.refresh_home_panel(&job, &refresh("3")).await.is_err());
    db.chat_finish(&job, "Updated personal panel", "complete")
        .await
        .unwrap();
    let state = panel(&db, &owner, &w).await;
    assert_eq!(state["active"]["definition"]["sections"][0]["value"], "2");
    assert_eq!(state["active_revision"], 1);
    assert_eq!(state["maintenance"]["status"], "current");
    assert_eq!(state["active"]["source_operations"], 1);
    let mut foreign_job = job.clone();
    foreign_job.requester = other.id;
    assert!(
        !super::source_access::allowed(&db.db, &foreign_job)
            .await
            .unwrap()
    );
    assert!(db.home_update_context(&original).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn home_maintenance_pause_fences_inflight_and_cancels_unstarted_manual_updates() {
    let (db, owner, w, _, read, _, p) = setup().await;
    db.save_home_agreement(&owner, &w, &p.id, &agreement(0, None))
        .await
        .unwrap();
    let request = update(1);
    let accepted = db
        .update_home_panel(&owner, &w, &p.id, &request)
        .await
        .unwrap();
    let job = claim(&db, &w, accepted["reply"].as_str().unwrap()).await;
    observed(&db, &job, &read).await;
    let mut pause = agreement(1, None);
    pause.enabled = false;
    db.save_home_agreement(&owner, &w, &p.id, &pause)
        .await
        .unwrap();
    assert!(db.refresh_home_panel(&job, &refresh("2")).await.is_err());
    assert!(
        db.control_propose(
            &job,
            &ActionRequest {
                connection: read.clone(),
                operation: "order.read".into(),
                arguments: json!({"order":"one"})
            }
        )
        .await
        .is_err()
    );
    assert_eq!(
        db.update_home_panel(&owner, &w, &p.id, &request)
            .await
            .unwrap(),
        accepted
    );
    assert_eq!(
        panel(&db, &owner, &w).await["active"]["definition"]["sections"][0]["value"],
        "1"
    );
    db.save_home_agreement(&owner, &w, &p.id, &agreement(2, None))
        .await
        .unwrap();
    let queued = db
        .update_home_panel(&owner, &w, &p.id, &update(3))
        .await
        .unwrap();
    let mut pause = agreement(3, None);
    pause.enabled = false;
    db.save_home_agreement(&owner, &w, &p.id, &pause)
        .await
        .unwrap();
    let row = query(
        &db.db,
        "SELECT state FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2",
        vec![w.clone().into(), queued["reply"].as_str().unwrap().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "finished");
    assert_eq!(
        panel(&db, &owner, &w).await["maintenance"]["status"],
        "paused"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn home_maintenance_schedule_coalesces_and_revocation_hides_saved_data() {
    let (db, owner, w, original, read, _, p) = setup().await;
    db.save_home_agreement(&owner, &w, &p.id, &agreement(0, Some(60)))
        .await
        .unwrap();
    let state = panel(&db, &owner, &w).await;
    let due = state["maintenance"]["next_ms"].as_i64().unwrap();
    let (one, two) = tokio::join!(db.dispatch_schedules_at(due), db.dispatch_schedules_at(due));
    assert_eq!(one.unwrap() + two.unwrap(), 1);
    let accepted = db
        .update_home_panel(&owner, &w, &p.id, &update(1))
        .await
        .unwrap();
    assert_eq!(db.dispatch_schedules_at(due + 120000).await.unwrap(), 0);
    let job = claim(&db, &w, accepted["reply"].as_str().unwrap()).await;
    observed(&db, &job, &read).await;
    db.refresh_home_panel(&job, &refresh("9")).await.unwrap();
    db.chat_finish(&job, "Updated", "complete").await.unwrap();
    let mut rule = Rule {
        connection: read,
        operation: "order.read".into(),
        mode: "block".into(),
        revision: 1,
        chat: original.chat.clone(),
        approver: None,
        approver_revision: None,
        approval_chat: None,
        delegation_revision: None,
    };
    db.control_rule(&owner, &w, &job.agent, &rule)
        .await
        .unwrap();
    assert_eq!(panel(&db, &owner, &w).await["active"]["available"], false);
    assert!(!super::source_access::allowed(&db.db, &job).await.unwrap());
    assert_eq!(db.dispatch_schedules_at(due + 180000).await.unwrap(), 0);
    assert!(
        db.update_home_panel(&owner, &w, &p.id, &update(1))
            .await
            .is_err()
    );
    rule.mode = "allow".into();
    rule.revision = 2;
    db.control_rule(&owner, &w, &job.agent, &rule)
        .await
        .unwrap();
    assert_eq!(panel(&db, &owner, &w).await["active"]["available"], false);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn home_maintenance_snapshots_exceed_definition_budget_with_bounded_independent_provenance() {
    let (db, owner, w, _, read, _, p) = setup().await;
    db.save_home_agreement(&owner, &w, &p.id, &agreement(0, None))
        .await
        .unwrap();
    for n in 1..=102 {
        let accepted = db
            .update_home_panel(&owner, &w, &p.id, &update(1))
            .await
            .unwrap();
        let job = claim(&db, &w, accepted["reply"].as_str().unwrap()).await;
        observed(&db, &job, &read).await;
        db.refresh_home_panel(&job, &refresh(&n.to_string()))
            .await
            .unwrap();
        db.chat_finish(&job, "Updated", "complete").await.unwrap();
    }
    let count=query(&db.db,"SELECT count(*) AS n,max(generation) AS generation FROM relay_home_snapshots WHERE workspace=$1 AND panel=$2",vec![w.clone().into(),p.id.clone().into()]).await.unwrap().unwrap();
    assert_eq!(count.try_get::<i64>("", "n").unwrap(), 32);
    assert_eq!(count.try_get::<i64>("", "generation").unwrap(), 102);
    let state = panel(&db, &owner, &w).await;
    assert_eq!(state["revision"], 1);
    assert_eq!(state["active"]["source_operations"], 1);
    assert_eq!(state["active"]["definition"]["sections"][0]["value"], "102");
}
