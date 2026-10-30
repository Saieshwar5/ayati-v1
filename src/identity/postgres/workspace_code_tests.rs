//! Invite-only early access: one code creates one workspace, and only while valid.
use super::{tests::*, *};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
async fn issue(db: &PgStore, admin: &User) -> String {
    db.issue_workspace_code(admin, "Mehta Furnishings", 14)
        .await
        .unwrap()["code"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn invalid(result: Result<String>) {
    assert_eq!(result.unwrap_err().to_string(), workspace_codes::INVALID);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_code_creates_exactly_one_workspace_and_is_recorded() {
    let db = fixture().await;
    let admin = user(&db, "codes-admin").await;
    let owner = user(&db, "codes-owner").await;
    let code = issue(&db, &admin).await;
    invalid(
        db.create_with_code(&owner, "Mehta", &id(), "CKO-AAAA-BBBB-CCCC")
            .await,
    );
    // Checking does not use the code; typing it in lower case with spaces still matches.
    db.check_workspace_code(&owner, &code.to_lowercase().replace('-', " "))
        .await
        .unwrap();
    let request = id();
    let workspace = db
        .create_with_code(&owner, "Mehta", &request, &code)
        .await
        .unwrap();
    // A retry of the same request returns the same workspace instead of failing.
    assert_eq!(
        db.create_with_code(&owner, "Mehta", &request, &code)
            .await
            .unwrap(),
        workspace
    );
    let other = user(&db, "codes-other").await;
    invalid(db.create_with_code(&other, "Copy", &id(), &code).await);
    invalid(db.create_with_code(&owner, "Second", &id(), &code).await);
    let listed = db.workspace_codes().await.unwrap();
    let entry = listed["codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["note"] == "Mehta Furnishings")
        .unwrap()
        .clone();
    assert_eq!(entry["status"], "used");
    assert_eq!(entry["workspace"]["id"], workspace.as_str());
    assert_eq!(entry["used_by"]["email"], owner.email.as_str());
    assert!(entry.get("code").is_none(), "codes are never listed again");
    assert!(
        db.revoke_workspace_code(&admin, entry["id"].as_str().unwrap())
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn expired_and_revoked_codes_create_nothing() {
    let db = fixture().await;
    let admin = user(&db, "codes-expiry-admin").await;
    let owner = user(&db, "codes-expiry-owner").await;
    let expired = issue(&db, &admin).await;
    execute(&db.db, "UPDATE relay_workspace_codes SET expires_at=1 WHERE code_hash=(SELECT code_hash FROM relay_workspace_codes ORDER BY created_at DESC, id LIMIT 1) AND used_at IS NULL", vec![]).await.unwrap();
    let revoked = db
        .issue_workspace_code(&admin, "Revoked company", 14)
        .await
        .unwrap();
    db.revoke_workspace_code(&admin, revoked["id"].as_str().unwrap())
        .await
        .unwrap();
    for code in [expired.as_str(), revoked["code"].as_str().unwrap()] {
        invalid(db.create_with_code(&owner, "Nope", &id(), code).await);
    }
    let owned = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_members WHERE user_id=$1",
        vec![owner.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<i64>("", "n")
    .unwrap();
    assert_eq!(owned, 0);
    assert!(db.issue_workspace_code(&admin, "", 14).await.is_err());
    assert!(
        db.issue_workspace_code(&admin, "Too long", 91)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn guessing_pauses_after_ten_failures_and_racing_users_get_one_workspace() {
    let db = fixture().await;
    let admin = user(&db, "codes-race-admin").await;
    let guesser = user(&db, "codes-guesser").await;
    let code = issue(&db, &admin).await;
    for _ in 0..10 {
        invalid(
            db.create_with_code(&guesser, "Guess", &id(), "CKO-ZZZZ-ZZZZ-ZZZZ")
                .await,
        );
    }
    let paused = db
        .create_with_code(&guesser, "Guess", &id(), &code)
        .await
        .unwrap_err();
    assert!(paused.to_string().contains("Too many attempts"));

    let (first, second) = (
        user(&db, "codes-race-one").await,
        user(&db, "codes-race-two").await,
    );
    let (one, two) = (id(), id());
    let (a, b) = tokio::join!(
        db.create_with_code(&first, "Race", &one, &code),
        db.create_with_code(&second, "Race", &two, &code)
    );
    assert_eq!(
        [a.is_ok(), b.is_ok()].iter().filter(|ok| **ok).count(),
        1,
        "{a:?} {b:?}"
    );
}
