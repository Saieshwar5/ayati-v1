//! Phone sign-in against real PostgreSQL and a fake WorkOS: the PKCE-bound
//! code and bearer requests.
use super::super::execute;
use super::test_fixture::*;
use crate::identity::{digest, random_token};

use serde_json::json;

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn a_code_works_once_and_only_with_the_apps_verifier() {
    let (_fake, base, server) = fake_workos().await;
    let app = app(&base, None).await;
    let verifier = random_token();
    let back = sign_in(&app, &digest(&verifier)).await;
    assert_eq!(
        back.as_str().split('?').next().unwrap(),
        format!("{ORIGIN}/auth/mobile/return")
    );
    let code = param(&back, "code").unwrap();

    // A wrong verifier is refused and burns the code: the right one then fails too.
    let (status, error) = redeem(&app, &code, &random_token()).await;
    assert_eq!(status, 400);
    assert_eq!(error["error"], super::INVALID_CODE);
    assert_eq!(redeem(&app, &code, &verifier).await.0, 400);

    // A good code works once.
    let verifier = random_token();
    let code = param(&sign_in(&app, &digest(&verifier)).await, "code").unwrap();
    assert_eq!(redeem(&app, &code, &verifier).await.0, 200);
    assert_eq!(redeem(&app, &code, &verifier).await.0, 400);

    // An expired code is refused like any other.
    let verifier = random_token();
    let code = param(&sign_in(&app, &digest(&verifier)).await, "code").unwrap();
    execute(
        &app.store.db,
        "UPDATE relay_mobile_codes SET expires=0",
        vec![],
    )
    .await
    .unwrap();
    assert_eq!(redeem(&app, &code, &verifier).await.0, 400);

    // The exchange still needs the request marker, and a malformed challenge
    // never reaches WorkOS.
    let body = json!({"code": code, "verifier": verifier, "device_name": "x", "platform": "ios"});
    let (status, _, _) = call(
        &app,
        "POST",
        "/api/identity/mobile-sessions",
        &[],
        Some(body),
    )
    .await;
    assert_eq!(status, 403);
    let (status, _, _) = call(&app, "GET", "/auth/mobile/start?challenge=short", &[], None).await;
    assert_eq!(status, 400);
    server.abort();
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn a_phone_reads_and_writes_with_its_bearer_token_until_workos_ends_it() {
    let (fake, base, server) = fake_workos().await;
    let app = app(&base, None).await;
    let (bearer, _) = phone(&app).await;
    let auth = [
        ("authorization", bearer.as_str()),
        ("x-ayati-request", "identity"),
    ];

    let (status, session, _) = call(&app, "GET", "/api/identity/session", &auth, None).await;
    assert_eq!(status, 200);
    assert_eq!(session["user"]["email"], "phone@example.test");
    // A change without Origin is accepted from a phone and checked live with WorkOS.
    let name = Some(json!({"display_name": "Asha"}));
    let (status, _, _) = call(&app, "POST", "/api/identity/profile", &auth, name.clone()).await;
    assert_eq!(status, 200);
    // The same request is a browser's once it carries a cookie: Origin is required.
    let with_cookie = [auth[0], auth[1], ("cookie", "ayati_identity=forged")];
    let (status, _, _) = call(
        &app,
        "POST",
        "/api/identity/profile",
        &with_cookie,
        name.clone(),
    )
    .await;
    assert_eq!(status, 403);
    // A forged token is no one.
    let forged = [
        ("authorization", "Bearer abcdefghijklmnopqrstuvwxyz012345"),
        auth[1],
    ];
    let (status, _, _) = call(&app, "POST", "/api/identity/profile", &forged, name.clone()).await;
    assert_eq!(status, 401);

    // WorkOS ends the session (blocked or signed out elsewhere): changes stop.
    fake.active.lock().unwrap().clear();
    let (status, _, _) = call(&app, "POST", "/api/identity/profile", &auth, name).await;
    assert_eq!(status, 401);
    // The device session itself expires after its 30 days.
    let (bearer, _) = phone(&app).await;
    execute(
        &app.store.db,
        "UPDATE relay_device_sessions SET expires=0",
        vec![],
    )
    .await
    .unwrap();
    let expired = [("authorization", bearer.as_str())];
    let (_, session, _) = call(&app, "GET", "/api/identity/session", &expired, None).await;
    assert!(
        session["user"].is_null() || session.get("error").is_some(),
        "{session}"
    );
    server.abort();
}
