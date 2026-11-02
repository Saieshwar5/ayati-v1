//! Living panels: the agent's work on a panel's sources and events on them update the
//! panel, merged, at most once per gap, never from a panel update itself, and only
//! while its agreement follows sources.
use super::super::super::activity::Filters;
use super::super::super::control_tests::request;
use super::super::super::home_maintenance_tests::{agreement, claim, observed, setup};
use super::super::super::tests::message;
use super::super::super::*;
use crate::identity::chat::model::Job;
use serde_json::json;

async fn wanted(db: &PgStore, panel: &str) -> Value {
    query(
        &db.db,
        "SELECT to_jsonb(k) AS v FROM relay_home_update_wanted k WHERE panel=$1",
        vec![panel.into()],
    )
    .await
    .unwrap()
    .map(|r| r.try_get("", "v").unwrap())
    .unwrap_or(Value::Null)
}

async fn runs(db: &PgStore, panel: &str) -> Vec<Value> {
    rows(
        &db.db,
        "SELECT to_jsonb(r) AS v FROM relay_home_update_runs r WHERE panel=$1 ORDER BY created_ms",
        vec![panel.into()],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.try_get("", "v").unwrap())
    .collect()
}

async fn due_now(db: &PgStore, panel: &str) {
    execute(
        &db.db,
        "UPDATE relay_home_update_wanted SET due_ms=1 WHERE panel=$1",
        vec![panel.into()],
    )
    .await
    .unwrap();
}

/// The owner asks the agent something; the agent does it and finishes.
async fn work(db: &PgStore, owner: &User, first: &Job, text: &str) -> Job {
    let sent = db
        .chat_send(
            owner,
            &first.workspace,
            &first.chat,
            &message(text, Some(&first.agent)),
        )
        .await
        .unwrap()
        .unwrap();
    claim(db, &first.workspace, &sent.reply).await
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn work_on_a_panels_source_updates_it_merged_and_at_most_once_per_gap() {
    let (db, owner, w, first, read, write, p) = setup().await;
    db.save_home_agreement(&owner, &w, &p.id, &agreement(0, None))
        .await
        .unwrap();

    // Work that only touched another connection wants nothing.
    let other = work(&db, &owner, &first, "Fix the address on order one").await;
    let action = db
        .control_propose(&other, &request(&write, "W-1"))
        .await
        .unwrap();
    let dispatch = action.dispatch.unwrap();
    db.control_ready(&w, &dispatch).await.unwrap();
    db.control_finish(&w, &dispatch, json!({"updated": true}), true)
        .await
        .unwrap();
    db.chat_finish(&other, "Fixed", "complete").await.unwrap();
    assert!(wanted(&db, &p.id).await.is_null());

    // Work that read the panel's source wants one update, and starts it.
    let orders = work(&db, &owner, &first, "How many orders came in?").await;
    observed(&db, &orders, &read).await;
    db.chat_finish(&orders, "Two orders", "complete")
        .await
        .unwrap();
    let want = wanted(&db, &p.id).await;
    assert_eq!(want["reason"], "work");
    assert_eq!(want["cause"], orders.reply.as_str());
    assert_eq!(db.dispatch_panel_updates().await.unwrap(), 1);
    assert!(wanted(&db, &p.id).await.is_null());
    let started = runs(&db, &p.id).await;
    assert_eq!(started.len(), 1);
    assert_eq!(started[0]["reason"], "work");
    let page = db
        .activity_history(
            &owner,
            &w,
            &Filters {
                kind: Some("panel".into()),
                ..Filters::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        page["items"][0]["summary"],
        "Updated panel: Personal order · after the agent's work"
    );

    // More work while that update runs merges into one wanted update that waits.
    for text in ["And now?", "And now, again?"] {
        let more = work(&db, &owner, &first, text).await;
        observed(&db, &more, &read).await;
        db.chat_finish(&more, "Checked", "complete").await.unwrap();
    }
    assert_eq!(db.dispatch_panel_updates().await.unwrap(), 0);
    let waiting = wanted(&db, &p.id).await;
    assert!(
        waiting["detail"]
            .as_str()
            .unwrap()
            .contains("current update")
    );

    // The update itself finishing never asks for another.
    let update = claim(&db, &w, started[0]["reply"].as_str().unwrap()).await;
    observed(&db, &update, &read).await;
    db.refresh_home_panel(
        &update,
        &super::super::super::home_maintenance_tests::refresh("3"),
    )
    .await
    .unwrap();
    db.chat_finish(&update, "Updated", "complete")
        .await
        .unwrap();
    assert_eq!(
        wanted(&db, &p.id).await["cause"],
        waiting["cause"],
        "still the one merged want"
    );

    // Within the gap (5 minutes by default) it waits for the gap's end, then runs.
    due_now(&db, &p.id).await;
    assert_eq!(db.dispatch_panel_updates().await.unwrap(), 0);
    let next = wanted(&db, &p.id).await;
    let gap_end = started[0]["created_ms"].as_i64().unwrap() + 300_000;
    assert_eq!(next["due_ms"].as_i64().unwrap(), gap_end);
    execute(
        &db.db,
        "UPDATE relay_home_update_runs SET created_ms=created_ms-400000 WHERE panel=$1",
        vec![p.id.clone().into()],
    )
    .await
    .unwrap();
    due_now(&db, &p.id).await;
    assert_eq!(db.dispatch_panel_updates().await.unwrap(), 1);
    assert_eq!(runs(&db, &p.id).await.len(), 2);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn events_on_a_panels_source_want_an_update_only_while_it_follows_sources() {
    let (db, owner, w, _, read, write, p) = setup().await;
    let mut quiet = agreement(0, None);
    quiet.follow_sources = Some(false);
    db.save_home_agreement(&owner, &w, &p.id, &quiet)
        .await
        .unwrap();
    let event = |connection: String, id: &'static str| {
        let db = &db;
        let w = w.clone();
        async move {
            let tx = db.db.begin().await.unwrap();
            super::on_event(&tx, &w, &connection, id).await.unwrap();
            tx.commit().await.unwrap();
        }
    };
    event(read.clone(), "evt-1").await;
    assert!(
        wanted(&db, &p.id).await.is_null(),
        "this panel doesn't follow its sources"
    );

    let mut follow = agreement(1, None);
    follow.follow_sources = Some(true);
    follow.min_gap_seconds = Some(120);
    db.save_home_agreement(&owner, &w, &p.id, &follow)
        .await
        .unwrap();
    let saved = db.home_panels(&owner, &w).await.unwrap()["panels"][0]["maintenance"].clone();
    assert_eq!(saved["min_gap_seconds"], 120);
    assert_eq!(saved["follow_sources"], true);
    event(write.clone(), "evt-2").await;
    assert!(
        wanted(&db, &p.id).await.is_null(),
        "an event on another connection"
    );
    event(read.clone(), "evt-3").await;
    event(read.clone(), "evt-4").await;
    let want = wanted(&db, &p.id).await;
    assert_eq!(
        (want["reason"].as_str(), want["cause"].as_str()),
        (Some("event"), Some("evt-3")),
        "merged"
    );

    // Paused updates drop what was wanted without starting anything.
    let mut paused = agreement(2, None);
    paused.enabled = false;
    db.save_home_agreement(&owner, &w, &p.id, &paused)
        .await
        .unwrap();
    assert_eq!(db.dispatch_panel_updates().await.unwrap(), 0);
    assert!(wanted(&db, &p.id).await.is_null());
    assert!(runs(&db, &p.id).await.is_empty());

    let mut fast = agreement(3, None);
    fast.min_gap_seconds = Some(30);
    assert!(
        db.save_home_agreement(&owner, &w, &p.id, &fast)
            .await
            .is_err()
    );
}
