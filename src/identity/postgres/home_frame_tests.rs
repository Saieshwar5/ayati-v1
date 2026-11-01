//! The panel frame (Phase 1e): the agent's "what changed" note travels with the
//! published update, is limited to 280 characters, and the panel names its latest
//! update so a delayed or failed panel can open it in Activity.
use super::super::home_maintenance_tests::{agreement, claim, observed, panel, setup, update};
use super::super::*;
use crate::identity::home_panels::maintenance::Refresh;
use serde_json::json;

fn refresh_with(value: &str, note: &str) -> Refresh {
    serde_json::from_value(json!({
        "sections": [{"kind": "metric", "label": "Order count", "value": value}],
        "what_changed": note,
    }))
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_update_carries_the_agents_short_note_and_the_panel_names_its_latest_update() {
    let (db, owner, w, _, read, _, p) = setup().await;
    db.save_home_agreement(&owner, &w, &p.id, &agreement(0, None))
        .await
        .unwrap();
    let reply = db
        .update_home_panel(&owner, &w, &p.id, &update(1))
        .await
        .unwrap()["reply"]
        .as_str()
        .unwrap()
        .to_string();
    let job = claim(&db, &w, &reply).await;
    observed(&db, &job, &read).await;

    // Over 280 characters is refused; the agent can shorten it and try again.
    let long = "x".repeat(281);
    let refused = db
        .refresh_home_panel(&job, &refresh_with("2", &long))
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("280 characters"), "{refused}");

    let note = "  Orders doubled since this morning: the Diwali listing went live.  ";
    db.refresh_home_panel(&job, &refresh_with("2", note))
        .await
        .unwrap();
    db.chat_finish(&job, "Updated personal panel", "complete")
        .await
        .unwrap();
    let state = panel(&db, &owner, &w).await;
    assert_eq!(
        state["active"]["what_changed"],
        "Orders doubled since this morning: the Diwali listing went live.",
        "kept, trimmed"
    );
    assert_eq!(state["maintenance"]["status"], "current");
    assert_eq!(state["maintenance"]["last_update"], reply.as_str());
    assert_eq!(
        state["active"]["task"],
        reply.as_str(),
        "Show proof opens this update"
    );

    // A blank note is no note: the next update shows nothing extra.
    let second = db
        .update_home_panel(&owner, &w, &p.id, &update(1))
        .await
        .unwrap()["reply"]
        .as_str()
        .unwrap()
        .to_string();
    let job = claim(&db, &w, &second).await;
    observed(&db, &job, &read).await;
    db.refresh_home_panel(&job, &refresh_with("3", "   "))
        .await
        .unwrap();
    db.chat_finish(&job, "Updated personal panel", "complete")
        .await
        .unwrap();
    let state = panel(&db, &owner, &w).await;
    assert_eq!(state["active"]["definition"]["sections"][0]["value"], "3");
    assert!(state["active"]["what_changed"].is_null());
    assert_eq!(state["maintenance"]["last_update"], second.as_str());
}
