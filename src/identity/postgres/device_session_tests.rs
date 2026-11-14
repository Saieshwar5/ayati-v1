//! Device sessions: one-time sign-in codes and the phone sessions they create.
use super::{device_sessions::INVALID_CODE, tests::*, *};
use crate::identity::provider::digest;
use serde_json::json;

async fn code(db: &PgStore, owner: &User) -> String {
    db.create_link_code(owner).await.unwrap().unwrap().0
}
fn invalid(result: Result<(String, Value)>) {
    assert_eq!(result.unwrap_err().to_string(), INVALID_CODE);
}
async fn audited(db: &PgStore, actor: &User, action: &str) -> i64 {
    query(
        &db.db,
        "SELECT count(*) AS n FROM relay_audit WHERE actor=$1 AND action=$2",
        vec![actor.id.clone().into(), action.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_code_works_once_and_a_new_code_replaces_the_old_one() {
    let db = fixture().await;
    let owner = user(&db, "device-owner").await;
    let first = code(&db, &owner).await;
    assert_eq!(first.len(), 43, "256 random bits, base64url");
    let second = code(&db, &owner).await;
    invalid(db.redeem_link_code(&first, "Ravi's iPhone", "ios").await);
    // Bad input is refused before the code is used up.
    assert!(db.redeem_link_code(&second, "", "ios").await.is_err());
    assert!(
        db.redeem_link_code(&second, &"x".repeat(61), "ios")
            .await
            .is_err()
    );
    assert!(
        db.redeem_link_code(&second, "Phone", "windows")
            .await
            .is_err()
    );
    let (token, device) = db
        .redeem_link_code(&second, "  Ravi's iPhone ", "ios")
        .await
        .unwrap();
    assert_eq!(device["name"], "Ravi's iPhone");
    assert_eq!(device["platform"], "ios");
    let lifetime = device["expires"].as_i64().unwrap() - device["created"].as_i64().unwrap();
    assert_eq!(lifetime, device_sessions::DEVICE_SECONDS);
    invalid(db.redeem_link_code(&second, "Again", "android").await);
    invalid(db.redeem_link_code("not-a-code", "Guess", "android").await);
    let session = db.device_session(&digest(&token)).await.unwrap().unwrap();
    assert_eq!(session.user.id, owner.id);
    assert_eq!(session.id, device["id"].as_str().unwrap());
    // Only the hash is stored.
    let stored = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_device_sessions WHERE token_hash=$1",
        vec![token.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<i64>("", "n")
    .unwrap();
    assert_eq!(stored, 0);
    assert_eq!(audited(&db, &owner, "device_code_created").await, 2);
    assert_eq!(audited(&db, &owner, "device_signed_in").await, 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn expired_codes_and_expired_devices_grant_nothing() {
    let db = fixture().await;
    let owner = user(&db, "device-expiry").await;
    let stale = code(&db, &owner).await;
    execute(
        &db.db,
        "UPDATE relay_device_link_codes SET expires=$1",
        vec![(now() - 1).into()],
    )
    .await
    .unwrap();
    invalid(db.redeem_link_code(&stale, "Phone", "android").await);
    let fresh = code(&db, &owner).await;
    let (token, _) = db
        .redeem_link_code(&fresh, "Phone", "android")
        .await
        .unwrap();
    execute(&db.db, "UPDATE relay_device_sessions SET expires=0", vec![])
        .await
        .unwrap();
    assert!(db.device_session(&digest(&token)).await.unwrap().is_none());
    let listed = db.devices(&owner, None).await.unwrap();
    assert_eq!(listed["devices"].as_array().unwrap().len(), 0);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn concurrent_redeems_of_one_code_sign_in_one_device() {
    let db = fixture().await;
    let owner = user(&db, "device-race").await;
    let shared = code(&db, &owner).await;
    let attempts = (0..8).map(|n| {
        let (db, shared) = (db.clone(), shared.clone());
        tokio::spawn(async move {
            db.redeem_link_code(&shared, &format!("Phone {n}"), "ios")
                .await
        })
    });
    let results = futures_util::future::join_all(attempts).await;
    let won = results
        .into_iter()
        .map(|result| result.unwrap())
        .filter(|result| match result {
            Ok(_) => true,
            Err(error) => {
                assert_eq!(error.to_string(), INVALID_CODE);
                false
            }
        })
        .count();
    assert_eq!(won, 1);
    let listed = db.devices(&owner, None).await.unwrap();
    assert_eq!(listed["devices"].as_array().unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn people_list_and_sign_out_only_their_own_devices() {
    let db = fixture().await;
    let owner = user(&db, "device-lister").await;
    let other = user(&db, "device-stranger").await;
    let (mine, device) = db
        .redeem_link_code(&code(&db, &owner).await, "Mine", "ios")
        .await
        .unwrap();
    let (_, theirs) = db
        .redeem_link_code(&code(&db, &other).await, "Theirs", "android")
        .await
        .unwrap();
    let listed = db.devices(&owner, Some(&digest(&mine))).await.unwrap();
    let devices = listed["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["name"], "Mine");
    assert_eq!(devices[0]["current"], true);
    assert!(devices[0].get("token").is_none() && devices[0].get("token_hash").is_none());
    let id = device["id"].as_str().unwrap();
    assert!(!db.revoke_device(&other, id).await.unwrap());
    assert!(
        !db.revoke_device(&owner, theirs["id"].as_str().unwrap())
            .await
            .unwrap()
    );
    assert!(db.device_session(&digest(&mine)).await.unwrap().is_some());
    assert!(db.revoke_device(&owner, id).await.unwrap());
    assert!(
        !db.revoke_device(&owner, id).await.unwrap(),
        "already signed out"
    );
    assert!(db.device_session(&digest(&mine)).await.unwrap().is_none());
    assert_eq!(
        db.devices(&owner, None).await.unwrap()["devices"],
        json!([])
    );
    assert_eq!(audited(&db, &owner, "device_signed_out").await, 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn code_creation_and_signed_in_devices_are_capped() {
    let db = fixture().await;
    let owner = user(&db, "device-caps").await;
    for _ in 0..10 {
        assert!(db.create_link_code(&owner).await.unwrap().is_some());
    }
    assert!(db.create_link_code(&owner).await.unwrap().is_none());
    // An hour later the person can make codes again.
    execute(
        &db.db,
        "UPDATE relay_device_link_codes SET window_start=window_start-3601",
        vec![],
    )
    .await
    .unwrap();
    assert!(db.create_link_code(&owner).await.unwrap().is_some());
    execute(
        &db.db,
        "UPDATE relay_device_link_codes SET window_count=0",
        vec![],
    )
    .await
    .unwrap();
    let mut first = None;
    for n in 0..20 {
        let (_, device) = db
            .redeem_link_code(&code(&db, &owner).await, &format!("Phone {n}"), "ios")
            .await
            .unwrap();
        first.get_or_insert(device);
        execute(
            &db.db,
            "UPDATE relay_device_link_codes SET window_count=0",
            vec![],
        )
        .await
        .unwrap();
    }
    let extra = code(&db, &owner).await;
    let refused = db.redeem_link_code(&extra, "One too many", "ios").await;
    assert!(
        refused
            .unwrap_err()
            .to_string()
            .starts_with("Too many devices")
    );
    // The refusal rolled back, so the same code works once a device signs out.
    let first = first.unwrap();
    assert!(
        db.revoke_device(&owner, first["id"].as_str().unwrap())
            .await
            .unwrap()
    );
    db.redeem_link_code(&extra, "Replacement", "ios")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn last_used_is_refreshed_only_by_touch() {
    let db = fixture().await;
    let owner = user(&db, "device-touch").await;
    let (token, _) = db
        .redeem_link_code(&code(&db, &owner).await, "Phone", "android")
        .await
        .unwrap();
    execute(
        &db.db,
        "UPDATE relay_device_sessions SET last_used=100",
        vec![],
    )
    .await
    .unwrap();
    let session = db.device_session(&digest(&token)).await.unwrap().unwrap();
    assert_eq!(session.last_used, 100, "reading the session writes nothing");
    db.touch_device(&session.id).await.unwrap();
    let session = db.device_session(&digest(&token)).await.unwrap().unwrap();
    assert!(session.last_used >= now() - 5);
}
