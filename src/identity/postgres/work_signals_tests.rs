//! Work is announced to the loops that act on it, at once and only for their
//! own schema; deadlines that nothing announces are returned instead.
use super::super::{
    executor_slot_tests::{finish, report, reserve, setup},
    tests::{chat, message},
    *,
};
use super::WorkSignals;
use crate::identity::{collaboration::Object, executor::model::Capacity, provider::digest};
use futures_util::FutureExt;
use std::sync::Arc;
use tokio::sync::Notify;

/// Far below every fallback timer (10, 30 and 60 seconds).
const PROMPTLY: Duration = Duration::from_secs(3);

fn woken(signal: &Notify) -> bool {
    signal.notified().now_or_never().is_some()
}
async fn wakes(signal: &Notify) -> bool {
    tokio::time::timeout(PROMPTLY, signal.notified())
        .await
        .is_ok()
}
/// Starts this store's listener and waits until it is connected (it wakes
/// every loop then), so later wake-ups come from notifications.
async fn listening(db: &PgStore) -> tokio::task::JoinHandle<Result<()>> {
    let listener = tokio::spawn({
        let db = db.clone();
        async move { db.listen_changes().await }
    });
    let work: &Arc<WorkSignals> = &db.work;
    for signal in [&work.dispatch, &work.schedule, &work.transcribe] {
        assert!(
            tokio::time::timeout(Duration::from_secs(10), signal.notified())
                .await
                .is_ok(),
            "connecting wakes every loop"
        );
    }
    listener
}

#[test]
fn a_notification_wakes_only_the_loop_it_names_in_its_own_schema() {
    let signals = WorkSignals::default();
    signals.notified("public", "public/dispatch");
    assert!(woken(&signals.dispatch));
    assert!(!woken(&signals.schedule) && !woken(&signals.transcribe));
    signals.notified("public", "relay_test_other/schedule");
    signals.notified("public", "publicx/schedule");
    assert!(!woken(&signals.schedule), "another schema's work");
    signals.notified("public", "public/transcribe");
    assert!(woken(&signals.transcribe) && !woken(&signals.dispatch));
    signals.notified("public", "public/something-new");
    assert!(woken(&signals.dispatch) && woken(&signals.schedule) && woken(&signals.transcribe));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn new_finished_and_ready_work_wakes_dispatch_without_waiting_for_the_fallback() {
    let (db, owner, w, c, a, e, slot) = setup().await;
    let listener = listening(&db).await;
    let work = db.work.clone();
    let capacity = Capacity {
        slots: vec![slot.clone()],
    };
    let changed = db.report_executor(&e.executor, &e.incarnation, &capacity);
    assert!(changed.await.unwrap(), "a new slot");
    assert!(wakes(&work.dispatch).await, "a slot became ready");
    // An unchanged heartbeat writes no slot row and announces nothing.
    let changed = db.report_executor(&e.executor, &e.incarnation, &capacity);
    assert!(!changed.await.unwrap(), "only a heartbeat");
    db.chat_send(&owner, &w, &c, &message("Announced request", Some(&a)))
        .await
        .unwrap();
    assert!(wakes(&work.dispatch).await, "a new job");
    assert!(
        wakes(&work.schedule).await,
        "a new job can be a direct panel update"
    );
    // The dispatcher's own rounds (recovery, a claim, an offer) never wake it
    // again, or an idle controller would spin.
    db.expire_executors(45).await.unwrap();
    let scope = reserve(&db, &e).await.unwrap();
    assert!(!wakes(&work.dispatch).await, "a claim is not new work");
    finish(&db, &slot, scope).await;
    assert!(wakes(&work.dispatch).await, "a finished job frees capacity");
    assert!(
        wakes(&work.schedule).await,
        "and the routine that waited for it"
    );
    listener.abort();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schedules_and_voice_notes_wake_their_loops_and_say_when_they_are_due() {
    let (db, owner, w, c, a) = super::super::automation_tests::setup().await;
    let listener = listening(&db).await;
    let work = db.work.clone();
    let routine = super::super::automation_tests::schedule(&c, Some(3600));
    db.save_schedule(&owner, &w, &a, &routine).await.unwrap();
    assert!(wakes(&work.schedule).await, "a new schedule");
    // Its first run is a minute away; the scheduler sleeps until then.
    let due = db.next_admission_in().await.unwrap().unwrap();
    assert!(
        due > Duration::from_secs(50) && due <= Duration::from_secs(60),
        "{due:?}"
    );

    let room = chat(&db, &owner, &w, vec![], vec![]).await;
    let voice = Object {
        id: uuid::Uuid::new_v4().to_string(),
        name: "voice-message.webm".into(),
        mime: "audio/webm".into(),
        size: 2048,
        checksum: "voicehash".into(),
    };
    db.save_object(&owner, &w, &room, &voice, true)
        .await
        .unwrap();
    assert!(wakes(&work.transcribe).await, "any controller may take it");
    assert_eq!(db.next_transcript_in().await.unwrap(), None, "takeable now");
    let mut taken = db.claim_transcripts(1).await.unwrap();
    let held = db.next_transcript_in().await.unwrap().unwrap();
    assert!(held > Duration::from_secs(140), "the lease: {held:?}");
    db.finish_transcript(
        &taken.remove(0),
        super::super::transcripts::Outcome::Failed { duration_ms: None },
    )
    .await
    .unwrap();
    assert!(
        wakes(&work.dispatch).await,
        "a request waiting for it may run"
    );
    listener.abort();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_lost_executor_is_detected_within_the_expiry_and_its_deadline_wakes_dispatch() {
    let (db, owner, w, c, a, e, slot) = setup().await;
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    let beat = |ago: i64| {
        let db = db.clone();
        let id = e.executor.clone();
        async move {
            execute(&db.db, "UPDATE relay_executors SET heartbeat=floor(extract(epoch FROM clock_timestamp()))::bigint-$2 WHERE id=$1",
                vec![id.into(), ago.into()]).await.unwrap();
        }
    };
    // Three heartbeats missed (10 s each) still leave it serving, and the
    // dispatcher is told when the default 45-second expiry passes.
    beat(35).await;
    let next = db.expire_executors(45).await.unwrap().unwrap();
    assert!(
        next > Duration::from_secs(8) && next <= Duration::from_secs(10),
        "{next:?}"
    );
    let credential = digest(&e.token);
    db.executor_allowed(&e.executor, &e.incarnation, &credential)
        .await
        .unwrap();
    // A queued request is offered now, and its startup lease is the next deadline.
    db.chat_send(&owner, &w, &c, &message("Before the expiry", Some(&a)))
        .await
        .unwrap();
    assert!(reserve(&db, &e).await.is_some());
    beat(44).await;
    let next = db.expire_executors(45).await.unwrap().unwrap();
    assert!(
        next <= Duration::from_secs(1),
        "the heartbeat expiry comes first: {next:?}"
    );
    beat(45).await;
    db.expire_executors(45).await.unwrap();
    assert!(
        report(&db, &e, vec![slot.clone()]).await.is_err(),
        "withdrawn"
    );
    let allowed = db.executor_allowed(&e.executor, &e.incarnation, &credential);
    assert!(allowed.await.is_err());
    assert!(reserve(&db, &e).await.is_none());
    // Its offer keeps its own lease; the dispatcher wakes when that runs out.
    let next = db.expire_executors(45).await.unwrap().unwrap();
    assert!(
        next > Duration::from_secs(10) && next <= Duration::from_secs(15),
        "{next:?}"
    );
}

/// Every table the loops act on announces its changes; a table recreated by a
/// later migration without its trigger would leave a loop on its fallback.
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn every_table_the_loops_act_on_announces_its_work() {
    let (db, ..) = setup().await;
    let announced: Vec<String> = rows(
        &db.db,
        "SELECT DISTINCT c.relname FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid
          JOIN pg_namespace n ON n.oid=c.relnamespace JOIN pg_proc p ON p.oid=t.tgfoid
         WHERE n.nspname=current_schema() AND p.proname='relay_announce_work' ORDER BY 1",
        vec![],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|row| row.try_get("", "relname").unwrap())
    .collect();
    let mut expected = [
        "relay_agent_handoffs",
        "relay_agent_schedules",
        "relay_built_app_events",
        "relay_chat_jobs",
        "relay_event_deliveries",
        "relay_executor_slots",
        "relay_executor_tasks",
        "relay_home_update_wanted",
        "relay_library_arrivals",
        "relay_object_transcripts",
        "relay_task_wakeups",
    ];
    expected.sort();
    assert_eq!(announced, expected);
}
