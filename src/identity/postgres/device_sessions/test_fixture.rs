//! A fake WorkOS and the phone app's requests, for the device-session tests.
use super::super::tests::fixture;
use crate::identity::{App, Provider, digest, random_token, router, workspaces::Runtimes};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Request, header},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

pub(super) const ORIGIN: &str = "http://127.0.0.1:8765";
pub(super) const HOST: &str = "127.0.0.1:8765";

/// WorkOS sessions it considers active, and the ones Cuckoo revoked.
#[derive(Default)]
pub(super) struct WorkOs {
    signed_in: Mutex<u32>,
    pub(super) active: Mutex<HashSet<String>>,
    pub(super) revoked: Mutex<Vec<String>>,
}

async fn authenticate(State(fake): State<Arc<WorkOs>>) -> Json<Value> {
    let number = {
        let mut count = fake.signed_in.lock().unwrap();
        *count += 1;
        *count
    };
    let sid = format!("session_phone{number}");
    fake.active.lock().unwrap().insert(sid.clone());
    let claims = URL_SAFE_NO_PAD.encode(json!({"sid": sid}).to_string());
    Json(json!({
        "user": {"id": "user_phone", "email": "phone@example.test", "email_verified": true},
        "access_token": format!("e30.{claims}.signature"),
    }))
}

pub(super) async fn fake_workos() -> (Arc<WorkOs>, String, tokio::task::JoinHandle<()>) {
    let fake = Arc::new(WorkOs::default());
    let routes = Router::new()
        .route("/user_management/authenticate", post(authenticate))
        .route(
            "/user_management/users/{id}",
            get(|| async { Json(json!({"email": "phone@example.test", "email_verified": true})) }),
        )
        .route(
            "/user_management/users/{id}/sessions",
            get(|State(fake): State<Arc<WorkOs>>| async move {
                let data: Vec<Value> = fake
                    .active
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|id| json!({"id": id, "status": "active"}))
                    .collect();
                Json(json!({"data": data}))
            }),
        )
        .route(
            "/user_management/sessions/revoke",
            post(
                |State(fake): State<Arc<WorkOs>>, Json(body): Json<Value>| async move {
                    let id = body["session_id"].as_str().unwrap().to_owned();
                    fake.active.lock().unwrap().remove(&id);
                    fake.revoked.lock().unwrap().push(id);
                    Json(json!({}))
                },
            ),
        )
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, routes).await.unwrap() });
    (fake, format!("http://{address}"), server)
}

pub(super) async fn app(base_url: &str, scheme: Option<&str>) -> Arc<App> {
    app_with(base_url, |mobile| mobile.scheme = scheme.map(str::to_owned)).await
}

pub(super) async fn app_with(
    base_url: &str,
    mobile: impl FnOnce(&mut crate::config::Mobile),
) -> Arc<App> {
    let mut config = crate::config::Controller::for_tests(ORIGIN);
    mobile(&mut config.mobile);
    Arc::new(App {
        library: Default::default(),
        provider: Provider::fixture(base_url.into()),
        store: fixture().await,
        config: Arc::new(config),
        runtimes: Runtimes::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string())),
        simulation: None,
    })
}

pub(super) async fn call(
    app: &Arc<App>,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> (u16, Value, HeaderMap) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", HOST);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let body = match body {
        Some(body) => {
            request = request.header("content-type", "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = router(app.clone())
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        headers,
    )
}

pub(super) fn location(headers: &HeaderMap) -> reqwest::Url {
    reqwest::Url::parse(headers[header::LOCATION].to_str().unwrap())
        .or_else(|_| reqwest::Url::parse(ORIGIN)?.join(headers[header::LOCATION].to_str().unwrap()))
        .unwrap()
}

pub(super) fn param(url: &reqwest::Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
}

/// The app's whole browser journey: start, WorkOS, callback. Returns the code.
pub(super) async fn sign_in(app: &Arc<App>, challenge: &str) -> reqwest::Url {
    let (status, _, headers) = call(
        app,
        "GET",
        &format!("/auth/mobile/start?challenge={challenge}"),
        &[],
        None,
    )
    .await;
    assert_eq!(status, 303);
    let authorize = location(&headers);
    assert_eq!(
        param(&authorize, "redirect_uri").unwrap(),
        format!("{ORIGIN}/auth/callback")
    );
    let state = param(&authorize, "state").unwrap();
    let flow = headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let (status, _, headers) = call(
        app,
        "GET",
        &format!("/auth/callback?code=workos-code&state={state}"),
        &[("cookie", &flow)],
        None,
    )
    .await;
    assert_eq!(status, 303);
    // A phone sign-in leaves no browser session behind.
    assert!(
        headers
            .get_all(header::SET_COOKIE)
            .iter()
            .all(|value| !value.to_str().unwrap().starts_with("ayati_identity=")),
    );
    location(&headers)
}

pub(super) async fn redeem(app: &Arc<App>, code: &str, verifier: &str) -> (u16, Value) {
    let body = json!({"code": code, "verifier": verifier, "device_name": "Test iPhone", "platform": "ios"});
    let (status, value, _) = call(
        app,
        "POST",
        "/api/identity/mobile-sessions",
        &[("x-ayati-request", "identity")],
        Some(body),
    )
    .await;
    (status, value)
}

/// A signed-in phone: its bearer header value and device id.
pub(super) async fn phone(app: &Arc<App>) -> (String, String) {
    let verifier = random_token();
    let back = sign_in(app, &digest(&verifier)).await;
    let (status, issued) = redeem(app, &param(&back, "code").unwrap(), &verifier).await;
    assert_eq!(status, 200, "{issued}");
    (
        format!("Bearer {}", issued["token"].as_str().unwrap()),
        issued["id"].as_str().unwrap().to_owned(),
    )
}
