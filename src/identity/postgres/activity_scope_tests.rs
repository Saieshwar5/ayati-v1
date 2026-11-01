//! Activity's "My agents" scope: requests the person made and the work of agents
//! they manage. It only narrows what the person may already see.
use super::super::tests::{chat, fixture, join, message, user, workspace};
use super::Filters as ActivityFilters;
use super::*;

fn scoped(scope: Option<&str>) -> ActivityFilters {
    ActivityFilters {
        scope: scope.map(str::to_string),
        ..ActivityFilters::default()
    }
}

async fn ids(db: &PgStore, person: &User, w: &str, f: &ActivityFilters) -> Vec<String> {
    let mut found: Vec<String> = db.activity_history(person, w, f).await.unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    found.sort();
    found
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

async fn everyone_may_talk(db: &PgStore, agent: &str) {
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET audience='everyone' WHERE id=$1",
        vec![agent.into()],
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn my_agents_shows_my_requests_and_the_agents_i_manage_only() {
    let db = fixture().await;
    let admin = user(&db, "scope-admin").await;
    let manager = user(&db, "scope-manager").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &manager, "Member").await;
    // Two agents created by the admin; the member manages only the first.
    let (admin_chat, mine) = super::super::executor_slot_tests::agent(&db, &admin, &w).await;
    let (_, other) = super::super::executor_slot_tests::agent(&db, &admin, &w).await;
    everyone_may_talk(&db, &mine).await;
    everyone_may_talk(&db, &other).await;
    execute(&db.db, "INSERT INTO relay_agent_managers(workspace,agent,user_id,added_by,at) VALUES($1,$2,$3,$4,1)", vec![w.clone().into(), mine.clone().into(), manager.id.clone().into(), admin.id.clone().into()]).await.unwrap();

    // The admin asks the managed agent in a chat the member is not in.
    let managed = db
        .chat_send(&admin, &w, &admin_chat, &message("Price list", Some(&mine)))
        .await
        .unwrap()
        .unwrap();
    // In a channel both are in, the member and the admin each ask the other agent.
    let shared = chat(
        &db,
        &admin,
        &w,
        vec![manager.id.clone()],
        vec![other.clone()],
    )
    .await;
    let own = db
        .chat_send(&manager, &w, &shared, &message("Stock count", Some(&other)))
        .await
        .unwrap()
        .unwrap();
    let not_mine = db
        .chat_send(&admin, &w, &shared, &message("Supplier list", Some(&other)))
        .await
        .unwrap()
        .unwrap();

    // All: everything the member may see. Mine: their own request and the work of
    // the agent they manage, but not the admin's request to the other agent.
    let all = sorted(vec![
        managed.reply.clone(),
        own.reply.clone(),
        not_mine.reply.clone(),
    ]);
    let narrowed = sorted(vec![managed.reply.clone(), own.reply.clone()]);
    assert_eq!(ids(&db, &manager, &w, &scoped(None)).await, all);
    assert_eq!(ids(&db, &manager, &w, &scoped(Some("all"))).await, all);
    assert_eq!(
        ids(&db, &manager, &w, &scoped(Some("mine"))).await,
        narrowed
    );

    // Totals and the export follow the same scope.
    let totals = db
        .activity_totals(&manager, &w, &scoped(Some("mine")))
        .await
        .unwrap();
    assert_eq!(totals["requests"], 2);
    let (csv, _) = db
        .activity_export(&manager, &w, &scoped(Some("mine")))
        .await
        .unwrap();
    assert_eq!(csv.lines().count(), 3, "a header and two requests");

    // Now: only the agents the member manages get a card, and running work is narrowed.
    let now = db
        .activity_now(&manager, &w, 1, Some("mine"))
        .await
        .unwrap();
    let cards: Vec<&str> = now["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(cards, vec![mine.as_str()]);
    let running: Vec<String> = now["running"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(sorted(running), narrowed);
    assert_eq!(now["away"]["totals"]["requests"], 2);
    let everything = db.activity_now(&manager, &w, 1, None).await.unwrap();
    assert_eq!(everything["agents"].as_array().unwrap().len(), 2);

    // An admin manages both agents (as their owner), so Mine keeps all three.
    assert_eq!(ids(&db, &admin, &w, &scoped(Some("mine"))).await, all);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_unknown_scope_is_refused() {
    let db = fixture().await;
    let admin = user(&db, "scope-refused").await;
    let w = workspace(&db, &admin).await;
    assert!(
        db.activity_history(&admin, &w, &scoped(Some("team")))
            .await
            .is_err()
    );
    assert!(db.activity_now(&admin, &w, 1, Some("team")).await.is_err());
}
