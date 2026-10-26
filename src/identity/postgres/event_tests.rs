use super::{tests::*, *};
use crate::identity::{
    control::{connections_model::SystemApproval, model::Rule},
    events::{
        catalog::Source,
        model::{Change, Trigger},
    },
};
use serde_json::json;
pub(super) struct Setup {
    pub db: PgStore,
    pub owner: User,
    pub w: String,
    pub c: String,
    pub a: String,
    pub source: Source,
    pub input: Trigger,
}
pub(super) async fn setup() -> Setup {
    let (db, owner, w, c, a) = super::automation_tests::setup().await;
    let conn = uuid::Uuid::new_v4().to_string();
    let fp = "a".repeat(43);
    db.control_system(
        &owner,
        &w,
        &SystemApproval {
            server: "events".into(),
            approved: true,
            revision: 0,
        },
    )
    .await
    .unwrap();
    db.control_connect(&owner,&w,&a,&conn,json!({"id":"events","name":"Orders","account":"reviewed@example.test","operations":[{"id":"order.read","title":"Read orders","tool":"get_order","effect":"read","schema":{"type":"object","properties":{},"additionalProperties":false}},{"id":"order.update","title":"Update order","tool":"update_order","effect":"write","schema":{"type":"object","properties":{"order":{"type":"string"},"address":{"type":"string"}},"required":["order","address"],"additionalProperties":false}}]}),&fp).await.unwrap();
    db.control_rule(&owner, &w, &a, &rule(&conn, &c, "allow", 0))
        .await
        .unwrap();
    let source = crate::identity::events::test_catalog::register(Source {
        id: uuid::Uuid::new_v4().to_string(),
        workspace: w.clone(),
        connection: conn,
        connection_fingerprint: fp,
        name: "Order changed".into(),
        provider: "composio_v3".into(),
        user_id: "person-1".into(),
        connected_account_id: "account-1".into(),
        auth_config_id: "auth-1".into(),
        trigger_id: "trigger-1".into(),
        trigger_slug: "ORDER_CHANGED".into(),
        read_operation: "order.read".into(),
        fields: vec!["order_id".into(), "status".into()],
        secret_file: String::new(),
    })
    .unwrap();
    let tx = db.read_tx(&w).await.unwrap();
    let v = super::event_access::versions(&tx, &owner, &w, &a, &c, &source)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let input = Trigger {
        id: uuid::Uuid::new_v4().to_string(),
        source: source.id.clone(),
        binding: source.fingerprint().unwrap(),
        chat: c.clone(),
        name: "Review order change".into(),
        instructions: "Review this change and report blockers.".into(),
        connection_revision: v["connection_revision"].as_i64().unwrap(),
        read_revision: v["read_revision"].as_i64().unwrap(),
        delegation_revision: None,
        filter: json!({"status":"delayed"}),
    };
    Setup {
        db,
        owner,
        w,
        c,
        a,
        source,
        input,
    }
}
pub(super) fn rule(conn: &str, c: &str, mode: &str, revision: i64) -> Rule {
    Rule {
        connection: conn.into(),
        operation: "order.read".into(),
        mode: mode.into(),
        revision,
        chat: c.into(),
        approver: None,
        approver_revision: None,
        approval_chat: None,
        delegation_revision: None,
    }
}
pub(super) fn payload(id: &str, status: &str) -> Value {
    json!({"id":id,"type":"composio.trigger.message","metadata":{"user_id":"person-1","connected_account_id":"account-1","auth_config_id":"auth-1","trigger_id":"trigger-1","trigger_slug":"ORDER_CHANGED"},"data":{"order_id":"PRIVATE-ORDER-42","status":status,"ignored_secret":"NEVER-COPY-THIS"},"timestamp":"2026-10-26T00:00:00Z"})
}
impl Setup {
    pub async fn save(&self) -> Value {
        self.db
            .save_event_trigger(
                &self.owner,
                &self.w,
                &self.a,
                &self.input,
                Some(&self.source),
            )
            .await
            .unwrap()
    }
    pub async fn accept(&self, id: &str, status: &str) -> Value {
        self.db
            .accept_event(&self.source, id, &payload(id, status), 100000)
            .await
            .unwrap()
    }
    pub async fn claim(&self) -> crate::identity::chat::store::Job {
        let mut job = self.job().await;
        job.claim = Some(uuid::Uuid::new_v4().to_string());
        execute(&self.db.db,"UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1",vec![job.reply.clone().into(),job.claim.clone().into()]).await.unwrap();
        job
    }
    pub async fn job(&self) -> crate::identity::chat::store::Job {
        let row=query(&self.db.db,"SELECT j.payload FROM relay_chat_jobs j JOIN relay_event_deliveries d ON d.workspace=j.workspace AND d.reply=j.reply ORDER BY j.sequence LIMIT 1",vec![]).await.unwrap().unwrap();
        serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap()
    }
    pub fn change(&self, revision: i64, enabled: bool) -> Change {
        Change {
            request: uuid::Uuid::new_v4().to_string(),
            revision,
            enabled,
            connection_revision: Some(self.input.connection_revision),
            read_revision: Some(self.input.read_revision),
            delegation_revision: None,
        }
    }
    pub async fn state(&self, input: &Change) -> Value {
        self.db
            .change_event_trigger(
                &self.owner,
                &self.w,
                &self.a,
                &self.input.id,
                input,
                std::slice::from_ref(&self.source),
            )
            .await
            .unwrap()
    }
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_exact_receipts_admit_one_atomic_principal_bound_job_and_isolate_context() {
    let s = setup().await;
    s.save().await;
    let event = payload("delivery-1", "delayed");
    let (one, two) = tokio::join!(
        s.db.accept_event(&s.source, "delivery-1", &event, 100000),
        s.db.accept_event(&s.source, "delivery-1", &event, 100000)
    );
    assert_ne!(one.unwrap()["duplicate"], two.unwrap()["duplicate"]);
    assert!(
        s.db.accept_event(
            &s.source,
            "delivery-1",
            &payload("delivery-1", "changed"),
            100000
        )
        .await
        .is_err()
    );
    let (one, two) = tokio::join!(s.db.dispatch_events(), s.db.dispatch_events());
    assert_eq!(one.unwrap() + two.unwrap(), 1);
    let job = s.job().await;
    assert_eq!(job.requester, s.owner.id);
    assert_eq!(job.chat, s.c);
    assert!(!job.prompt.contains("PRIVATE-ORDER"));
    let sources = s.db.event_job_context(&job).await.unwrap();
    assert!(json!(sources).to_string().contains("PRIVATE-ORDER-42"));
    assert!(!json!(sources).to_string().contains("NEVER-COPY"));
    let mut claimed = job.clone();
    claimed.claim = Some(uuid::Uuid::new_v4().to_string());
    execute(&s.db.db,"UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1",vec![job.reply.clone().into(),claimed.claim.clone().into()]).await.unwrap();
    s.db.chat_finish(&claimed, "PRIVATE-EVENT-RESULT", "complete")
        .await
        .unwrap();
    let next =
        s.db.chat_send(
            &s.owner,
            &s.w,
            &s.c,
            &message("Independent next task", Some(&s.a)),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(!next.prompt.contains("PRIVATE-EVENT-RESULT"));
    assert!(s.db.event_job_context(&next).await.unwrap().is_empty());
    assert_eq!(
        s.db.response_body(&s.owner, &s.w, &s.c, &job.reply, false)
            .await
            .unwrap()["source_scope"],
        s.source.id
    );
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    assert!(
        query(
            &s.db.db,
            "SELECT id FROM relay_event_inbox WHERE data IS NOT NULL",
            vec![]
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(s.accept("delivery-1", "delayed").await["duplicate"], true);
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_unmatched_paused_and_stopped_receipts_never_rematch_or_restart() {
    let s = setup().await;
    s.accept("before-rule", "delayed").await;
    s.save().await;
    s.accept("not-matched", "on-time").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    let pause = s.change(1, false);
    s.state(&pause).await;
    s.accept("while-paused", "delayed").await;
    let resume = s.change(2, true);
    s.state(&resume).await;
    assert_eq!(s.state(&pause).await["state"], "paused"); // Old acknowledgment never reapplies pause.
    s.accept("before-rule", "delayed").await;
    s.accept("while-paused", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    s.accept("fresh-event", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let job = s.job().await;
    s.db.cancel_assignment(&s.owner, &s.w, &s.c, &job.reply)
        .await
        .unwrap();
    s.accept("fresh-event", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_pressure_has_no_partial_ack_and_two_distinct_ids_wait_without_overlap() {
    let s = setup().await;
    s.save().await;
    s.accept("one", "delayed").await;
    assert!(
        s.db.accept_event(&s.source, "two", &payload("two", "delayed"), 1)
            .await
            .unwrap_err()
            .is::<crate::identity::events::model::InboxFull>()
    );
    assert!(
        query(
            &s.db.db,
            "SELECT id FROM relay_event_inbox WHERE id='two'",
            vec![]
        )
        .await
        .unwrap()
        .is_none()
    );
    s.accept("two", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    let job = s.job().await;
    s.db.cancel_assignment(&s.owner, &s.w, &s.c, &job.reply)
        .await
        .unwrap();
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    assert_eq!(
        s.db.event_runs(&s.owner, &s.w, &s.a, &s.input.id)
            .await
            .unwrap()["runs"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
