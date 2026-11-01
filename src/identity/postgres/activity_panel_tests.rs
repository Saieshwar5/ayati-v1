//! Home panel refreshes in Activity: their own type, named after the panel, and
//! visible to the panel's owner, the agent's managers and admins only.
use super::super::home_maintenance_tests::{agreement, setup, update};
use super::super::tests::{join, user};
use super::super::*;
use super::Filters;

fn kind(kind: &str) -> Filters {
    Filters {
        kind: Some(kind.into()),
        ..Filters::default()
    }
}

async fn ids(db: &PgStore, person: &User, w: &str, filters: &Filters) -> Vec<String> {
    db.activity_history(person, w, filters).await.unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn panel_updates_are_their_own_type_and_stay_with_the_panels_owner() {
    let (db, owner, w, _, _, _, p) = setup().await;
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

    // Named after its panel and filed as a panel update, not as a chat request.
    let page = db
        .activity_history(&owner, &w, &kind("panel"))
        .await
        .unwrap();
    let item = page["items"][0].clone();
    assert_eq!(item["id"], reply.as_str());
    assert_eq!(item["kind"], "panel");
    assert_eq!(item["panel"]["id"], p.id.as_str());
    assert!(
        item["summary"]
            .as_str()
            .unwrap()
            .starts_with("Updated panel: "),
        "{item}"
    );
    assert!(!ids(&db, &owner, &w, &kind("chat")).await.contains(&reply));
    let details = db.activity_request(&owner, &w, &reply).await.unwrap();
    assert_eq!(details["kind"], "panel");
    assert_eq!(details["summary"], item["summary"]);

    // Someone else in the panel's source conversation doesn't see a personal
    // panel's updates, in History or by opening the request directly.
    let member = user(&db, "panel-neighbour").await;
    join(&db, &owner, &w, &member, "Member").await;
    let chat: String = query(
        &db.db,
        "SELECT chat FROM relay_home_panels WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), p.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "chat")
    .unwrap();
    execute(
        &db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
        vec![w.clone().into(), chat.into(), member.id.clone().into()],
    )
    .await
    .unwrap();
    assert!(
        !ids(&db, &member, &w, &Filters::default())
            .await
            .contains(&reply)
    );
    assert!(db.activity_request(&member, &w, &reply).await.is_err());
}
