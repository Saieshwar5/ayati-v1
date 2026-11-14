//! Signed-in phones: listing, revocation, sign-out, membership and the
//! development return scheme, against real PostgreSQL and a fake WorkOS.
use super::super::{execute, tests::*};
use super::test_fixture::*;
use crate::identity::{digest, random_token};
use axum::http::header;

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn leaving_a_workspace_cuts_the_phone_off_too() {
    let (_fake, base, server) = fake_workos().await;
    let app = app(&base, None).await;
    let (bearer, _) = phone(&app).await;
    let auth = [("authorization", bearer.as_str())];
    let owner = user(&app.store, "owner").await;
    let w = workspace(&app.store, &owner).await;
    let phone_user = app
        .store
        .device_session(&digest(bearer.strip_prefix("Bearer ").unwrap()))
        .await
        .unwrap()
        .unwrap()
        .0;
    execute(
        &app.store.db,
        "INSERT INTO relay_members(workspace,user_id,role) VALUES($1,$2,'Admin')",
        vec![w.clone().into(), phone_user.id.clone().into()],
    )
    .await
    .unwrap();
    let people = format!("/api/identity/workspaces/{w}/people");
    assert_eq!(call(&app, "GET", &people, &auth, None).await.0, 200);
    execute(
        &app.store.db,
        "DELETE FROM relay_members WHERE workspace=$1 AND user_id=$2",
        vec![w.into(), phone_user.id.into()],
    )
    .await
    .unwrap();
    assert_ne!(call(&app, "GET", &people, &auth, None).await.0, 200);
    server.abort();
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn devices_are_listed_revoked_and_signed_out() {
    let (fake, base, server) = fake_workos().await;
    let app = app(&base, None).await;
    let (first, first_id) = phone(&app).await;
    let (second, _) = phone(&app).await;
    let marker = ("x-ayati-request", "identity");

    let (status, list, _) = call(
        &app,
        "GET",
        "/api/identity/devices",
        &[("authorization", &second)],
        None,
    )
    .await;
    assert_eq!(status, 200);
    let devices = list["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices.iter().filter(|d| d["current"] == true).count(), 1);
    assert_eq!(devices[0]["name"], "Test iPhone");

    // The second phone signs the first one out, and its WorkOS session ends.
    let revoke = format!("/api/identity/devices/{first_id}/revoke");
    let (status, result, _) = call(
        &app,
        "POST",
        &revoke,
        &[("authorization", &second), marker],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(result["provider_session_revoked"], true);
    assert_eq!(fake.revoked.lock().unwrap().as_slice(), ["session_phone1"]);
    let (_, session, _) = call(
        &app,
        "GET",
        "/api/identity/session",
        &[("authorization", &first)],
        None,
    )
    .await;
    assert!(
        session["user"].is_null() || session.get("error").is_some(),
        "{session}"
    );
    // Revoking again, or someone else's device, finds nothing.
    let (status, _, _) = call(
        &app,
        "POST",
        &revoke,
        &[("authorization", &second), marker],
        None,
    )
    .await;
    assert_eq!(status, 400);

    // Signing out deletes the phone's own session.
    let (status, _, _) = call(
        &app,
        "POST",
        "/api/identity/logout",
        &[("authorization", &second), marker],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(fake.revoked.lock().unwrap().len(), 2);
    let (status, _, _) = call(
        &app,
        "GET",
        "/api/identity/devices",
        &[("authorization", &second)],
        None,
    )
    .await;
    assert_eq!(status, 401);
    server.abort();
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn a_development_build_gets_its_scheme_and_a_failed_sign_in_returns_to_the_app() {
    let (_fake, base, server) = fake_workos().await;
    let app = app(&base, Some("cuckoo-dev")).await;
    let verifier = random_token();
    let back = sign_in(&app, &digest(&verifier)).await;
    assert_eq!(back.scheme(), "cuckoo-dev");
    assert_eq!(
        redeem(&app, &param(&back, "code").unwrap(), &verifier)
            .await
            .0,
        200
    );

    let (_, _, headers) = call(
        &app,
        "GET",
        &format!("/auth/mobile/start?challenge={}", digest("x")),
        &[],
        None,
    )
    .await;
    let flow = headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let (_, _, headers) = call(
        &app,
        "GET",
        "/auth/callback?code=c&state=wrong",
        &[("cookie", &flow)],
        None,
    )
    .await;
    let failed = location(&headers);
    assert_eq!(failed.scheme(), "cuckoo-dev");
    assert_eq!(param(&failed, "error").as_deref(), Some("sign_in_failed"));

    // No app identities configured: the universal-link files are not served.
    for path in [
        "/.well-known/apple-app-site-association",
        "/.well-known/assetlinks.json",
    ] {
        assert_eq!(call(&app, "GET", path, &[], None).await.0, 404);
    }
    let fingerprint = vec!["AB"; 32].join(":");
    let app = app_with(&base, |mobile| {
        mobile.ios_app_ids = vec!["ABCDE12345.app.cuckoo.mobile".into()];
        mobile.android = Some(("app.cuckoo.mobile".into(), vec![fingerprint.clone()]));
    })
    .await;
    let (status, apple, headers) = call(
        &app,
        "GET",
        "/.well-known/apple-app-site-association",
        &[],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    assert_eq!(
        apple["applinks"]["details"][0]["appIDs"][0],
        "ABCDE12345.app.cuckoo.mobile"
    );
    assert_eq!(
        apple["applinks"]["details"][0]["components"][0]["/"],
        "/auth/mobile/return"
    );
    let (status, android, _) = call(&app, "GET", "/.well-known/assetlinks.json", &[], None).await;
    assert_eq!(status, 200);
    assert_eq!(android[0]["target"]["package_name"], "app.cuckoo.mobile");
    assert_eq!(
        android[0]["target"]["sha256_cert_fingerprints"][0],
        fingerprint.as_str()
    );
    server.abort();
}
