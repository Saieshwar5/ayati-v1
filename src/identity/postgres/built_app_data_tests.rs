//! Built app data: the manifest of the running version and the person's roles decide
//! every read and write; preview records stay apart from live ones.
use super::super::super::tests::{fixture, join, user, workspace};
use super::super::{AppFile, AppRelease, NewApp};
use super::{DataRequest, RoleChange};
use crate::identity::User;
use crate::identity::postgres::PgStore;
use serde_json::{Value, json};

const MANIFEST: &str = r#"{"name":"Leave","roles":["user","approver"],"collections":{"requests":{"fields":{"start":"date","days":"number","reason":"text","approved":"boolean"},
    "access":{"user":["create","read_own","update_own"],"approver":["read","update"]}}}}"#;

fn ask(op: &str, record: Option<&str>, data: Option<Value>, revision: Option<i64>) -> DataRequest {
    DataRequest {
        preview: false,
        op: op.into(),
        collection: "requests".into(),
        record: record.map(str::to_owned),
        data,
        revision,
        mine: false,
    }
}
async fn data(
    db: &PgStore,
    who: &User,
    w: &str,
    id: &str,
    request: DataRequest,
) -> anyhow::Result<Value> {
    db.built_app_data(who, w, id, &request).await
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn app_data_follows_the_manifest_and_the_roles_people_hold() {
    let db = fixture().await;
    let admin = user(&db, "data-admin").await;
    let ravi = user(&db, "data-ravi").await;
    let mina = user(&db, "data-mina").await;
    let zoe = user(&db, "data-zoe").await;
    let w = workspace(&db, &admin).await;
    for person in [&ravi, &mina, &zoe] {
        join(&db, &admin, &w, person, "Member").await;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let app = NewApp {
        id: id.clone(),
        name: "Leave".into(),
        description: String::new(),
        audience: "everyone".into(),
        people: vec![],
        template: String::new(),
    };
    db.built_app_create(&ravi, &w, &app).await.unwrap();
    let file = AppFile {
        path: "cuckoo-app.json".into(),
        content: Some(MANIFEST.into()),
        revision: 1,
    };
    db.built_app_write(&ravi, &w, &id, &file).await.unwrap();
    // Before publishing, nobody uses live data; the builder previews with every role.
    assert!(
        data(&db, &mina, &w, &id, ask("me", None, None, None))
            .await
            .is_err()
    );
    let mut preview = ask("create", None, Some(json!({"days":1})), None);
    preview.preview = true;
    data(&db, &ravi, &w, &id, preview).await.unwrap();
    let mut me = ask("me", None, None, None);
    me.preview = true;
    assert_eq!(
        data(&db, &ravi, &w, &id, me).await.unwrap()["roles"],
        json!(["user", "approver"])
    );
    db.built_app_publish(
        &ravi,
        &w,
        &id,
        &AppRelease {
            note: String::new(),
            revision: 2,
        },
    )
    .await
    .unwrap();
    // Everyone holds the first role: they create and see only their own requests.
    assert_eq!(
        data(&db, &mina, &w, &id, ask("me", None, None, None))
            .await
            .unwrap()["roles"],
        json!(["user"])
    );
    let made = data(
        &db,
        &mina,
        &w,
        &id,
        ask(
            "create",
            None,
            Some(json!({"start":"2026-11-10","days":2,"reason":"Trip"})),
            None,
        ),
    )
    .await
    .unwrap();
    assert_eq!(made["mine"], true);
    assert_eq!(made["revision"], 1);
    let record = made["id"].as_str().unwrap().to_owned();
    data(
        &db,
        &zoe,
        &w,
        &id,
        ask("create", None, Some(json!({"days":1})), None),
    )
    .await
    .unwrap();
    let theirs = data(&db, &mina, &w, &id, ask("list", None, None, None))
        .await
        .unwrap();
    assert_eq!(
        theirs["records"].as_array().unwrap().len(),
        1,
        "only Mina's own, and no preview records"
    );
    assert!(
        data(&db, &zoe, &w, &id, ask("get", Some(&record), None, None))
            .await
            .is_err()
    );
    assert!(
        data(
            &db,
            &zoe,
            &w,
            &id,
            ask("update", Some(&record), Some(json!({"days":9})), Some(1))
        )
        .await
        .is_err()
    );
    // The manifest's fields are enforced, and nobody may delete.
    let bad = data(
        &db,
        &mina,
        &w,
        &id,
        ask("create", None, Some(json!({"salary":1})), None),
    )
    .await
    .unwrap_err();
    assert!(bad.to_string().contains("no field \"salary\""));
    assert!(
        data(
            &db,
            &mina,
            &w,
            &id,
            ask("create", None, Some(json!({"start":"next week"})), None)
        )
        .await
        .is_err()
    );
    let denied = data(
        &db,
        &mina,
        &w,
        &id,
        ask("delete", Some(&record), None, None),
    )
    .await
    .unwrap_err();
    assert!(denied.to_string().contains("can't delete requests"));
    // Only the builder or an admin gives roles; the approver reads and updates all.
    let approver = RoleChange {
        role: "approver".into(),
        people: vec![zoe.id.clone()],
    };
    assert!(
        db.built_app_roles_set(&mina, &w, &id, &approver)
            .await
            .is_err()
    );
    assert!(
        db.built_app_roles_set(
            &ravi,
            &w,
            &id,
            &RoleChange {
                role: "user".into(),
                people: vec![]
            }
        )
        .await
        .is_err()
    );
    db.built_app_roles_set(&ravi, &w, &id, &approver)
        .await
        .unwrap();
    assert_eq!(
        db.built_app_roles(&admin, &w, &id).await.unwrap()["held"][0]["id"],
        zoe.id.as_str()
    );
    assert_eq!(
        data(&db, &zoe, &w, &id, ask("list", None, None, None))
            .await
            .unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let mut mine = ask("list", None, None, None);
    mine.mine = true;
    assert_eq!(
        data(&db, &zoe, &w, &id, mine).await.unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let approved = data(
        &db,
        &zoe,
        &w,
        &id,
        ask(
            "update",
            Some(&record),
            Some(json!({"approved":true})),
            Some(1),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        approved["data"],
        json!({"start":"2026-11-10","days":2,"reason":"Trip","approved":true})
    );
    assert_eq!(approved["mine"], false);
    // A change from an old revision is refused.
    let stale = data(
        &db,
        &mina,
        &w,
        &id,
        ask("update", Some(&record), Some(json!({"days":3})), Some(1)),
    )
    .await
    .unwrap_err();
    assert!(
        stale
            .downcast_ref::<crate::identity::library::model::Conflict>()
            .is_some()
    );
    data(
        &db,
        &mina,
        &w,
        &id,
        ask("update", Some(&record), Some(json!({"days":3})), Some(2)),
    )
    .await
    .unwrap();
    // Unknown collections and operations are refused.
    let mut other = ask("list", None, None, None);
    other.collection = "salaries".into();
    assert!(data(&db, &mina, &w, &id, other).await.is_err());
    assert!(
        data(&db, &mina, &w, &id, ask("drop", None, None, None))
            .await
            .is_err()
    );
    // Taken offline, its data can't be reached.
    db.built_app_rollback(
        &admin,
        &w,
        &id,
        &AppRelease {
            note: String::new(),
            revision: 0,
        },
    )
    .await
    .unwrap();
    assert!(
        data(&db, &zoe, &w, &id, ask("list", None, None, None))
            .await
            .is_err()
    );
}
