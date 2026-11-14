//! Device sessions over HTTP: the web makes a code, a phone redeems it and then
//! uses its bearer token through the same authorization as cookies. WorkOS is a
//! loopback fixture.
use super::{tests::*, *};
use crate::identity::{self, App, Provider, digest};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::Path,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::get,
};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

const ORIGIN: &str = "https://workspace.example.test";

/// WorkOS knows `user_<name>` as `<name>@example.test`, with web session
/// `session_web`; `user_gone` was deleted.
async fn workos() -> String {
    let router = Router::new()
        .route(
            "/user_management/users/{id}",
            get(|Path(id): Path<String>| async move {
                match id.strip_prefix("user_") {
                    Some("gone") | None => StatusCode::NOT_FOUND.into_response(),
                    Some(name) => {
                        Json(json!({"email":format!("{name}@example.test"),"email_verified":true}))
                            .into_response()
                    }
                }
            }),
        )
        .route(
            "/user_management/users/{id}/sessions",
            get(|| async { Json(json!({"data":[{"id":"session_web","status":"active"}]})) }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async { axum::serve(listener, router).await.unwrap() });
    base
}
async fn app() -> Arc<App> {
    let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    Arc::new(App {
        library: Default::default(),
        provider: Provider::fixture(workos().await),
        store: fixture().await,
        config: Arc::new(crate::config::Controller::for_tests(ORIGIN)),
        runtimes: identity::workspaces::Runtimes::new(root),
        simulation: None,
    })
}
/// A person signed in on the web; their cookie value is `<name>-cookie`.
async fn web_user(app: &App, name: &str) -> User {
    let cookie = format!("{name}-cookie");
    app.store
        .login(
            &format!("user_{name}"),
            &format!("{name}@example.test"),
            &digest(&cookie),
            "session_web",
        )
        .await
        .unwrap()
}
enum As<'a> {
    Browser(&'a str),
    Phone(&'a str),
}
async fn call(app: &Arc<App>, who: As<'_>, method: &str, path: &str, body: Value) -> (u16, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "workspace.example.test")
        .header("content-type", "application/json");
    request = match who {
        // The web client's own requests carry the Origin check headers.
        As::Browser(name) => request
            .header("cookie", format!("ayati_identity={name}-cookie"))
            .header("origin", ORIGIN)
            .header("x-ayati-request", "identity"),
        // The phone app sends neither a cookie nor an Origin.
        As::Phone("") => request,
        As::Phone(token) => request.header("authorization", format!("Bearer {token}")),
    };
    let body = if body.is_null() {
        Body::empty()
    } else {
        Body::from(body.to_string())
    };
    let response = identity::router(app.clone())
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
/// The web makes a code; the phone redeems it. Returns the phone's token and device ID.
async fn sign_in_phone(app: &Arc<App>, name: &str) -> (String, String) {
    let (status, made) = call(
        app,
        As::Browser(name),
        "POST",
        "/api/identity/devices/link-code",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{made}");
    let redeem =
        json!({"code":made["code"],"device_name":format!("{name}'s phone"),"platform":"ios"});
    let (status, signed_in) = call(
        app,
        As::Phone(""),
        "POST",
        "/api/identity/devices/redeem",
        redeem,
    )
    .await;
    assert_eq!(status, 200, "{signed_in}");
    (
        signed_in["token"].as_str().unwrap().to_owned(),
        signed_in["device"]["id"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_phone_signs_in_with_a_code_and_acts_as_its_person() {
    let app = app().await;
    let owner = web_user(&app, "ravi").await;
    let w = workspace(&app.store, &owner).await;
    let (status, made) = call(
        &app,
        As::Browser("ravi"),
        "POST",
        "/api/identity/devices/link-code",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    let code = made["code"].as_str().unwrap();
    assert_eq!(made["expires_in"], 600);
    assert_eq!(
        made["link"],
        format!("cuckoo://sign-in?server=https%3A%2F%2Fworkspace.example.test&code={code}")
    );
    assert!(
        made["qr"]
            .as_str()
            .unwrap()
            .starts_with("data:image/svg+xml;base64,")
    );
    let redeem = json!({"code":code,"device_name":"Ravi's iPhone","platform":"ios"});
    let (status, signed_in) = call(
        &app,
        As::Phone(""),
        "POST",
        "/api/identity/devices/redeem",
        redeem.clone(),
    )
    .await;
    assert_eq!(status, 200);
    let token = signed_in["token"].as_str().unwrap();
    assert_eq!(signed_in["device"]["name"], "Ravi's iPhone");
    // Used once; a second scan gets the generic refusal.
    let (status, again) = call(
        &app,
        As::Phone(""),
        "POST",
        "/api/identity/devices/redeem",
        redeem,
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(again["error"], device_sessions::INVALID_CODE);

    let (status, session) = call(
        &app,
        As::Phone(token),
        "GET",
        "/api/identity/session",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(session["user"]["email"], "ravi@example.test");
    assert_eq!(session["workspaces"][0]["id"], w.as_str());
    let people = format!("/api/identity/workspaces/{w}/people");
    assert_eq!(
        call(&app, As::Phone(token), "GET", &people, Value::Null)
            .await
            .0,
        200
    );
    // A change without Origin works for a bearer request ...
    let rename = json!({"display_name":"Ravi Kumar"});
    let (status, renamed) = call(
        &app,
        As::Phone(token),
        "POST",
        "/api/identity/profile",
        rename.clone(),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(renamed["display_name"], "Ravi Kumar");
    // ... but a cookie without the browser's Origin headers is still refused.
    let request = Request::builder()
        .method("POST")
        .uri("/api/identity/profile")
        .header("host", "workspace.example.test")
        .header("content-type", "application/json")
        .header("cookie", "ayati_identity=ravi-cookie")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(rename.to_string()))
        .unwrap();
    let response = identity::router(app.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let (status, listed) = call(
        &app,
        As::Phone(token),
        "GET",
        "/api/identity/devices",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(listed["devices"][0]["current"], true);
    assert!(listed["devices"][0].get("token").is_none());
    let (_, listed) = call(
        &app,
        As::Browser("ravi"),
        "GET",
        "/api/identity/devices",
        Value::Null,
    )
    .await;
    assert_eq!(listed["devices"][0]["current"], false);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_phone_cannot_make_codes_and_bad_tokens_get_401() {
    let app = app().await;
    web_user(&app, "meera").await;
    let (token, _) = sign_in_phone(&app, "meera").await;
    let (status, _) = call(
        &app,
        As::Phone(&token),
        "POST",
        "/api/identity/devices/link-code",
        Value::Null,
    )
    .await;
    assert_eq!(
        status, 403,
        "a signed-in phone must not sign in more phones"
    );
    let forged = "A".repeat(43);
    for bad in [forged.as_str(), "short", ""] {
        let (status, _) = call(
            &app,
            As::Phone(bad),
            "GET",
            "/api/identity/session",
            Value::Null,
        )
        .await;
        let expected = if bad.is_empty() { 200 } else { 401 };
        assert_eq!(status, expected, "token {bad:?}");
    }
    let (status, _) = call(
        &app,
        As::Phone(&forged),
        "GET",
        "/api/identity/devices",
        Value::Null,
    )
    .await;
    assert_eq!(status, 401);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn signing_out_expiry_and_a_deleted_account_end_the_phone_session() {
    let app = app().await;
    let owner = web_user(&app, "asha").await;
    let other = web_user(&app, "kiran").await;
    let (token, device) = sign_in_phone(&app, "asha").await;
    let (other_token, other_device) = sign_in_phone(&app, "kiran").await;
    // Nobody signs out someone else's phone.
    let revoke_other = format!("/api/identity/devices/{other_device}/revoke");
    assert_eq!(
        call(
            &app,
            As::Browser("asha"),
            "POST",
            &revoke_other,
            Value::Null
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(&app, As::Phone(&token), "POST", &revoke_other, Value::Null)
            .await
            .0,
        404
    );
    assert_eq!(
        call(
            &app,
            As::Phone(&other_token),
            "GET",
            "/api/identity/session",
            Value::Null
        )
        .await
        .0,
        200
    );
    // The phone signs itself out.
    let revoke = format!("/api/identity/devices/{device}/revoke");
    assert_eq!(
        call(&app, As::Phone(&token), "POST", &revoke, Value::Null)
            .await
            .0,
        200
    );
    assert_eq!(
        call(
            &app,
            As::Phone(&token),
            "GET",
            "/api/identity/session",
            Value::Null
        )
        .await
        .0,
        401
    );
    // The web signs the other person's phone out after expiry: it already stopped.
    execute(
        &app.store.db,
        "UPDATE relay_device_sessions SET expires=0 WHERE user_id=$1",
        vec![other.id.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(
        call(
            &app,
            As::Phone(&other_token),
            "GET",
            "/api/identity/session",
            Value::Null
        )
        .await
        .0,
        401
    );
    // Web cookies keep working throughout.
    let (status, session) = call(
        &app,
        As::Browser("asha"),
        "GET",
        "/api/identity/session",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(session["user"]["id"], owner.id.as_str());

    // A WorkOS account that was deleted signs its phone out.
    let gone = app
        .store
        .login(
            "user_gone",
            "gone@example.test",
            &digest("gone-cookie"),
            "session_web",
        )
        .await
        .unwrap();
    let (code, _) = app.store.create_link_code(&gone).await.unwrap().unwrap();
    let (gone_token, _) = app
        .store
        .redeem_link_code(&code, "Old phone", "android")
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            As::Phone(&gone_token),
            "GET",
            "/api/identity/session",
            Value::Null
        )
        .await
        .0,
        401
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_phone_loses_a_workspace_when_its_person_is_removed() {
    let app = app().await;
    let owner = web_user(&app, "owner").await;
    let member = web_user(&app, "member").await;
    let w = workspace(&app.store, &owner).await;
    join(&app.store, &owner, &w, &member, "Member").await;
    let (token, _) = sign_in_phone(&app, "member").await;
    let preferences = format!("/api/identity/workspaces/{w}/preferences");
    assert_eq!(
        call(&app, As::Phone(&token), "GET", &preferences, Value::Null)
            .await
            .0,
        200
    );
    // Roles apply as on the web: a member cannot see or manage people.
    let people = format!("/api/identity/workspaces/{w}/people");
    assert_eq!(
        call(&app, As::Phone(&token), "GET", &people, Value::Null)
            .await
            .0,
        400
    );
    let remove_owner = format!("/api/identity/workspaces/{w}/members/{}", owner.id);
    assert_eq!(
        call(&app, As::Phone(&token), "POST", &remove_owner, json!({}))
            .await
            .0,
        400
    );
    app.store
        .change(&owner, &w, &member.id, None, None)
        .await
        .unwrap();
    let (status, _) = call(&app, As::Phone(&token), "GET", &preferences, Value::Null).await;
    assert_ne!(status, 200);
    let (_, session) = call(
        &app,
        As::Phone(&token),
        "GET",
        "/api/identity/session",
        Value::Null,
    )
    .await;
    assert_eq!(session["workspaces"], json!([]));
}
