//! Activity › Upcoming: the people who look after an agent see what it will do on its
//! own; a schedule posting in a conversation is seen only by people in it.
use crate::identity::postgres::automation_tests::{schedule, setup};
use crate::identity::postgres::tests::{chat, join, user};
use crate::identity::postgres::{AppRelease, AppWatch, NewApp};
use serde_json::Value;

fn titles(page: &Value, kind: &str) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == kind)
        .map(|i| i["title"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn upcoming_lists_what_runs_on_its_own_for_the_people_who_may_see_it() {
    let (db, owner, w, _, agent) = setup().await;
    let admin = user(&db, "upcoming-admin").await;
    let member = user(&db, "upcoming-member").await;
    join(&db, &owner, &w, &admin, "Member").await;
    crate::identity::postgres::execute(
        &db.db,
        "UPDATE relay_members SET role='Admin' WHERE workspace=$1 AND user_id=$2",
        vec![w.clone().into(), admin.id.clone().into()],
    )
    .await
    .unwrap();
    join(&db, &owner, &w, &member, "Member").await;

    let mut open = schedule("unused", Some(86_400));
    open.chat = None;
    open.name = "Morning stock check".into();
    db.save_schedule(&owner, &w, &agent, &open).await.unwrap();
    // Posts in the owner's own conversation with the agent, which the admin isn't in.
    let private_chat = chat(&db, &owner, &w, vec![], vec![agent.clone()]).await;
    let mut private = schedule(&private_chat, None);
    private.name = "Private summary".into();
    db.save_schedule(&owner, &w, &agent, &private)
        .await
        .unwrap();
    let paused = db
        .change_schedule(
            &owner,
            &w,
            &agent,
            &private.id,
            &crate::identity::automations::model::Change {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                enabled: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(paused["state"], "paused");

    // An app trigger on the agent.
    let app = uuid::Uuid::new_v4().to_string();
    let new_app = NewApp {
        id: app.clone(),
        name: "Leave".into(),
        description: String::new(),
        audience: "everyone".into(),
        people: vec![],
        template: "leave".into(),
    };
    db.built_app_create(&owner, &w, &new_app).await.unwrap();
    let release = AppRelease {
        note: String::new(),
        revision: 1,
    };
    db.built_app_publish(&owner, &w, &app, &release)
        .await
        .unwrap();
    let watch = AppWatch {
        collection: "requests".into(),
        kind: "added".into(),
        agent: Some(agent.clone()),
        instructions: "Tell me who asked.".into(),
    };
    db.built_app_watch_set(&owner, &w, &app, &watch)
        .await
        .unwrap();

    let mine = db.activity_upcoming(&owner, &w, None).await.unwrap();
    let mut listed = titles(&mine, "schedule");
    listed.sort();
    assert_eq!(
        listed,
        vec!["Morning stock check", "Private summary"],
        "a paused one still shows"
    );
    let private_row = mine["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["title"] == "Private summary")
        .unwrap();
    assert_eq!(private_row["state"], "paused");
    assert_eq!(private_row["own"], true);
    assert_eq!(titles(&mine, "app_trigger"), vec!["New requests in Leave"]);

    let theirs = db.activity_upcoming(&admin, &w, Some("all")).await.unwrap();
    assert_eq!(
        titles(&theirs, "schedule"),
        vec!["Morning stock check"],
        "a schedule posting where the admin isn't stays private"
    );
    assert_eq!(
        theirs["items"][0]["own"], false,
        "only its person resumes it"
    );
    let narrowed = db
        .activity_upcoming(&admin, &w, Some("mine"))
        .await
        .unwrap();
    assert!(
        titles(&narrowed, "schedule").is_empty(),
        "My agents: only agents the admin manages"
    );
    let outsider = db.activity_upcoming(&member, &w, None).await.unwrap();
    assert!(outsider["items"].as_array().unwrap().is_empty());
    assert!(
        db.activity_upcoming(&owner, &w, Some("everyone"))
            .await
            .is_err()
    );
}
