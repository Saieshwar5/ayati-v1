//! Built apps: the builder changes a draft and publishes it once the checks pass;
//! people open only the published version they were given; admins roll back.
use super::super::tests::{fixture, join, user, workspace};
use super::{AppFile, AppRelease, NewApp};

fn new(id: &str, audience: &str, people: Vec<String>) -> NewApp {
    NewApp {
        id: id.into(),
        name: "Leave requests".into(),
        description: "Ask for time off".into(),
        audience: audience.into(),
        people,
    }
}
fn write(path: &str, content: Option<&str>, revision: i64) -> AppFile {
    AppFile {
        path: path.into(),
        content: content.map(str::to_owned),
        revision,
    }
}
fn release(revision: i64) -> AppRelease {
    AppRelease {
        note: "First".into(),
        revision,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn built_apps_publish_after_checks_and_open_only_for_chosen_people() {
    let db = fixture().await;
    let admin = user(&db, "built-admin").await;
    let ravi = user(&db, "built-ravi").await;
    let mina = user(&db, "built-mina").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &ravi, "Member").await;
    join(&db, &admin, &w, &mina, "Member").await;
    let id = uuid::Uuid::new_v4().to_string();
    // Any member starts an app; repeating the same create is harmless.
    db.built_app_create(&ravi, &w, &new(&id, "selected", vec![mina.id.clone()]))
        .await
        .unwrap();
    db.built_app_create(&ravi, &w, &new(&id, "selected", vec![mina.id.clone()]))
        .await
        .unwrap();
    let draft = db.built_app_files(&ravi, &w, &id, 0).await.unwrap();
    assert_eq!(draft["files"].as_array().unwrap().len(), 5);
    assert_eq!(draft["checks"], serde_json::json!([]));
    // Before publishing, Mina sees nothing and can't open it; the admin can preview.
    let view = db.workspace_apps(&mina, &w).await.unwrap();
    assert!(view["built"].as_array().unwrap().is_empty());
    assert!(db.built_app_run(&mina, &w, &id, false).await.is_err());
    assert!(db.built_app_run(&admin, &w, &id, true).await.is_ok());
    assert!(db.built_app_files(&mina, &w, &id, 0).await.is_err());
    // Only the builder writes, from the revision they read.
    assert!(
        db.built_app_write(&admin, &w, &id, &write("app.js", Some("x"), 1))
            .await
            .is_err()
    );
    assert!(
        db.built_app_write(&ravi, &w, &id, &write("../x", Some("x"), 1))
            .await
            .is_err()
    );
    let key = "const key = 'sk-abcdefghijklmnopqrstuvwxyz0123456789';";
    let changed = db
        .built_app_write(&ravi, &w, &id, &write("config.js", Some(key), 1))
        .await
        .unwrap();
    assert_eq!(changed["draft_revision"], 2);
    let stale = db
        .built_app_write(&ravi, &w, &id, &write("app.js", Some("x"), 1))
        .await
        .unwrap_err();
    assert!(
        stale
            .downcast_ref::<crate::identity::library::model::Conflict>()
            .is_some()
    );
    // A key in the code blocks publishing; removing the file clears it.
    let blocked = db
        .built_app_publish(&ravi, &w, &id, &release(2))
        .await
        .unwrap();
    assert_eq!(blocked["state"], "checks_failed");
    assert_eq!(blocked["checks"][0]["check"], "secrets");
    db.built_app_write(&ravi, &w, &id, &write("config.js", None, 2))
        .await
        .unwrap();
    assert!(
        db.built_app_publish(&admin, &w, &id, &release(3))
            .await
            .is_err()
    );
    let published = db
        .built_app_publish(&ravi, &w, &id, &release(3))
        .await
        .unwrap();
    assert_eq!(published["version"], 1);
    // Mina was chosen and opens version 1; the admin's own view always lists it.
    let run = db.built_app_run(&mina, &w, &id, false).await.unwrap();
    assert_eq!(run["version"], 1);
    assert!(
        run["document"]
            .as_str()
            .unwrap()
            .contains("default-src 'none'")
    );
    let theirs = &db.workspace_apps(&mina, &w).await.unwrap()["built"][0];
    assert_eq!(theirs["can_open"], true);
    assert_eq!(theirs["can_manage"], false);
    assert!(theirs["versions"].is_null(), "only managers see versions");
    // Draft changes stay out of the published version until the next publish.
    db.built_app_write(
        &ravi,
        &w,
        &id,
        &write("app.js", Some("document.title='v2';"), 3),
    )
    .await
    .unwrap();
    assert!(
        !db.built_app_run(&mina, &w, &id, false).await.unwrap()["document"]
            .as_str()
            .unwrap()
            .contains("v2")
    );
    db.built_app_publish(&ravi, &w, &id, &release(4))
        .await
        .unwrap();
    assert!(
        db.built_app_run(&mina, &w, &id, false).await.unwrap()["document"]
            .as_str()
            .unwrap()
            .contains("v2")
    );
    // The admin rolls back to version 1, then takes it offline.
    assert!(
        db.built_app_rollback(&mina, &w, &id, &release(1))
            .await
            .is_err()
    );
    db.built_app_rollback(&admin, &w, &id, &release(1))
        .await
        .unwrap();
    assert_eq!(
        db.built_app_run(&mina, &w, &id, false).await.unwrap()["version"],
        1
    );
    db.built_app_rollback(&admin, &w, &id, &release(0))
        .await
        .unwrap();
    assert!(db.built_app_run(&mina, &w, &id, false).await.is_err());
    // Settings: for everyone; a download is a ZIP of the version's files.
    db.built_app_rollback(&ravi, &w, &id, &release(2))
        .await
        .unwrap();
    let mut open = new(&id, "everyone", vec![]);
    open.name = "Time off".into();
    db.built_app_settings(&ravi, &w, &id, &open).await.unwrap();
    let (name, zip) = db.built_app_download(&ravi, &w, &id, 2).await.unwrap();
    assert_eq!(name, "time-off-v2.zip");
    assert!(zip.starts_with(b"PK"));
    assert!(db.built_app_download(&mina, &w, &id, 2).await.is_err());
    let view = db.workspace_apps(&admin, &w).await.unwrap();
    assert_eq!(view["built"][0]["versions"].as_array().unwrap().len(), 2);
    // Removing hides it from everyone.
    assert!(db.built_app_remove(&mina, &w, &id).await.is_err());
    db.built_app_remove(&ravi, &w, &id).await.unwrap();
    assert!(
        db.workspace_apps(&admin, &w).await.unwrap()["built"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(db.built_app_run(&admin, &w, &id, true).await.is_err());
}
