//! Team panels: only a panel whose numbers all come from the source can be shared;
//! viewers see checked numbers but not the agent's words from a conversation they
//! can't see; a shared panel refuses numbers that don't match its source; the owner
//! narrows it again; each viewer places it on their own Home.
use super::super::super::home_maintenance_tests::{agreement, claim, setup, update};
use super::super::super::tests::{chat, join, user};
use super::super::super::*;
use crate::identity::home_panels::model::Share;
use crate::identity::{
    chat::model::Job, control::model::ActionRequest, home_panels::model::Layout,
};
use serde_json::json;

async fn read(db: &PgStore, job: &Job, connection: &str, count: i64) {
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
    db.control_finish(&job.workspace, &dispatch, json!({"count": count}), true)
        .await
        .unwrap();
}

async fn update_now(db: &PgStore, owner: &User, w: &str, panel: &str) -> Job {
    let revision = db.home_panels(owner, w).await.unwrap()["panels"][0]["maintenance"]["revision"]
        .as_i64()
        .unwrap();
    let reply = db
        .update_home_panel(owner, w, panel, &update(revision))
        .await
        .unwrap()["reply"]
        .as_str()
        .unwrap()
        .to_string();
    // With bindings saved, the controller claims the update itself.
    match db.direct_pick().await.unwrap().pop() {
        Some(job) => job,
        None => claim(db, w, &reply).await,
    }
}

fn publish(value: &str, bind: bool) -> crate::identity::home_panels::maintenance::Refresh {
    let mut input = json!({"sections":[{"kind":"metric","label":"Order count","value":value}]});
    if bind {
        input["bindings"] = json!([{"target":"value","read":0,"path":"count"}]);
    }
    serde_json::from_value(input).unwrap()
}

fn share(revision: i64, audience: &str, chat: Option<&str>) -> Share {
    Share {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        audience: audience.into(),
        chat: chat.map(str::to_string),
    }
}

async fn seen(db: &PgStore, person: &User, w: &str) -> Vec<Value> {
    db.home_panels(person, w).await.unwrap()["panels"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["shared"] == true)
        .cloned()
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn only_source_bound_panels_are_shared_and_viewers_see_only_checked_numbers() {
    let (db, owner, w, first, connection, _, p) = setup().await;
    let member = user(&db, "team-panel-member").await;
    let other = user(&db, "team-panel-other").await;
    join(&db, &owner, &w, &member, "Member").await;
    join(&db, &owner, &w, &other, "Member").await;

    // Not shareable until its updates run and every number is bound to the source.
    let mine = db.home_panels(&owner, &w).await.unwrap()["panels"][0].clone();
    assert_eq!(mine["audience"], "personal");
    assert_eq!(mine["shareable"]["ok"], false);
    assert!(
        db.share_home_panel(&owner, &w, &p.id, &share(0, "workspace", None))
            .await
            .is_err()
    );
    db.save_home_agreement(&owner, &w, &p.id, &agreement(0, None))
        .await
        .unwrap();
    let job = update_now(&db, &owner, &w, &p.id).await;
    read(&db, &job, &connection, 2).await;
    db.refresh_home_panel(&job, &publish("2", true))
        .await
        .unwrap();
    db.chat_finish(&job, "Updated", "complete").await.unwrap();
    assert_eq!(
        db.home_panels(&owner, &w).await.unwrap()["panels"][0]["shareable"]["ok"],
        true
    );

    // Everyone: only for an agent everyone may use; the revision must be current.
    let selected = db
        .share_home_panel(&owner, &w, &p.id, &share(0, "workspace", None))
        .await
        .unwrap_err();
    assert!(
        selected.to_string().contains("everyone may use"),
        "{selected}"
    );
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET audience='everyone' WHERE id=$1",
        vec![first.agent.clone().into()],
    )
    .await
    .unwrap();
    assert!(
        db.share_home_panel(&owner, &w, &p.id, &share(5, "workspace", None))
            .await
            .is_err()
    );
    let saved = db
        .share_home_panel(&owner, &w, &p.id, &share(0, "workspace", None))
        .await
        .unwrap();
    assert_eq!(saved["audience_revision"], 1);

    // A member sees the checked number, read-only, without the conversation's words.
    let shown = seen(&db, &member, &w).await;
    assert_eq!(shown.len(), 1);
    assert_eq!(
        shown[0]["active"]["definition"]["sections"][0]["value"],
        "2"
    );
    assert!(shown[0]["owner_name"].as_str().is_some());
    assert!(
        shown[0]["active"]["task"].is_null(),
        "no proof link into a conversation they can't see"
    );
    assert!(shown[0]["active"]["what_changed"].is_null());
    // ...and arranges it on their own Home.
    db.save_home_layout(
        &member,
        &w,
        &Layout {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            order: vec![p.id.clone()],
            wide: vec![],
            hidden: vec![],
        },
    )
    .await
    .unwrap();

    // A shared panel refuses numbers that don't match its source.
    let job = update_now(&db, &owner, &w, &p.id).await;
    read(&db, &job, &connection, 2).await;
    let refused = db
        .refresh_home_panel(&job, &publish("5", false))
        .await
        .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("only numbers read from the source"),
        "{refused}"
    );
    db.refresh_home_panel(&job, &publish("2", false))
        .await
        .unwrap();
    db.chat_finish(&job, "Updated", "complete").await.unwrap();

    // A channel: its members only.
    let room = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec![first.agent.clone()],
    )
    .await;
    db.share_home_panel(&owner, &w, &p.id, &share(1, "channel", Some(&room)))
        .await
        .unwrap();
    assert_eq!(seen(&db, &member, &w).await.len(), 1);
    assert!(seen(&db, &other, &w).await.is_empty(), "not in the channel");
    assert!(
        db.save_home_layout(
            &other,
            &w,
            &Layout {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 0,
                order: vec![p.id.clone()],
                wide: vec![],
                hidden: vec![]
            }
        )
        .await
        .is_err()
    );
    let mine = db.home_panels(&owner, &w).await.unwrap()["panels"][0].clone();
    assert_eq!(
        (mine["audience"].as_str(), mine["audience_chat"].as_str()),
        (Some("channel"), Some(room.as_str()))
    );

    // Narrowing back to the owner hides it from everyone else.
    db.share_home_panel(&owner, &w, &p.id, &share(2, "personal", None))
        .await
        .unwrap();
    assert!(seen(&db, &member, &w).await.is_empty());
    // Only the owner chooses.
    assert!(
        db.share_home_panel(&member, &w, &p.id, &share(3, "workspace", None))
            .await
            .is_err()
    );
}
