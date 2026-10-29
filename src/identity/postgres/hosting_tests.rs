//! Hosted trust/restart boundaries with real PostgreSQL and a loopback identity fixture.
use super::{tests::*, *};
use crate::identity::{
    self, App, Pending, Provider,
    agents::config::{SaveAgent, Settings},
};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tower::ServiceExt;
const ORIGIN: &str = "https://workspace.example.test";

async fn provider() -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let jwt = format!(
        "fixture.{}.fixture",
        URL_SAFE_NO_PAD.encode(br#"{"sid":"session_hosted"}"#)
    );
    let router = Router::new()
        .route("/user_management/authenticate", post(move |Json(input): Json<Value>| async move {
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(input["code"], "fixture-code");
            assert!(input["code_verifier"].as_str().unwrap().len() >= 43);
            Json(serde_json::json!({"user":{"id":"user_hosted","email":"owner@example.test","email_verified":true},"access_token":jwt}))
        }))
        .route("/user_management/users/user_hosted", get(|| async { Json(serde_json::json!({"email":"owner@example.test","email_verified":true})) }))
        .route("/user_management/users/user_hosted/sessions", get(|| async { Json(serde_json::json!({"data":[{"id":"session_hosted","status":"active"}]})) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async { axum::serve(listener, router).await.unwrap() });
    (base, count, server)
}
fn app(db: PgStore, root: &Path, provider: &str, storage: identity::library::Storage) -> Arc<App> {
    Arc::new(App {
        library: storage,
        provider: Provider::fixture(provider.into()),
        store: db,
        origin: ORIGIN.into(),
        runtimes: identity::workspaces::Runtimes::new(root.join("workspaces")),
        simulation: None,
    })
}
async fn reopen(db: &PgStore) -> PgStore {
    let schema: String = query(&db.db, "SELECT current_schema() AS name", vec![])
        .await
        .unwrap()
        .unwrap()
        .try_get("", "name")
        .unwrap();
    let mut url = reqwest::Url::parse(&std::env::var("RELAY_TEST_DATABASE_URL").unwrap()).unwrap();
    url.set_username("relay_app").unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    PgStore::connect(url.as_str()).await.unwrap()
}
async fn call(
    app: &Arc<App>,
    method: &str,
    path: &str,
    cookie: &str,
    body: Vec<u8>,
) -> (u16, axum::http::HeaderMap, Vec<u8>) {
    let response = identity::router(app.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("host", "workspace.example.test")
                .header("origin", ORIGIN)
                .header("x-ayati-request", "identity")
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    (
        status,
        headers,
        to_bytes(response.into_body(), 16 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn sign_in_survives_api_replacement_but_state_and_sessions_cannot_be_replayed() {
    let db = fixture().await;
    let replacement = reopen(&db).await;
    let root = tempfile::tempdir().unwrap();
    let (provider, calls, server) = provider().await;
    let first = app(db, root.path(), &provider, Default::default());
    let (status, headers, _) = call(&first, "GET", "/auth/login", "", vec![]).await;
    assert_eq!(status, 303);
    let flow = headers["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let url = reqwest::Url::parse(headers["location"].to_str().unwrap()).unwrap();
    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    drop(first);
    let second = app(replacement, root.path(), &provider, Default::default());
    let callback = format!("/auth/callback?code=fixture-code&state={state}");
    let (status, headers, _) = call(&second, "GET", &callback, &flow, vec![]).await;
    assert_eq!(status, 303);
    assert_eq!(headers["location"], "/?view=account");
    let session = headers
        .get_all("set-cookie")
        .iter()
        .map(|h| h.to_str().unwrap())
        .find(|s| s.starts_with("ayati_identity="))
        .unwrap();
    assert!(session.contains("; Secure"));
    let cookie = session.split(';').next().unwrap().to_owned();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        call(&second, "GET", &callback, &flow, vec![]).await.1["location"]
            .to_str()
            .unwrap()
            .contains("login_error")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // A separate API connection and empty scratch root retain the durable session.
    let new_root = tempfile::tempdir().unwrap();
    let third = app(
        reopen(&second.store).await,
        new_root.path(),
        &provider,
        Default::default(),
    );
    let (status, _, bytes) = call(&third, "GET", "/api/identity/session", &cookie, vec![]).await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["user"]["email"],
        "owner@example.test"
    );
    let hash = identity::digest(flow.split_once('=').unwrap().1);
    let pending = Pending {
        state: "expected".into(),
        verifier: "verifier".into(),
        invitation: None,
        expires: now() + 60,
    };
    third.store.save_login_flow(&hash, &pending).await.unwrap();
    call(
        &third,
        "GET",
        "/auth/callback?code=unused&state=wrong",
        &flow,
        vec![],
    )
    .await;
    assert!(third.store.take_login_flow(&hash).await.unwrap().is_none());
    let mut expired = pending;
    expired.expires = now() - 1;
    third.store.save_login_flow(&hash, &expired).await.unwrap();
    call(
        &third,
        "GET",
        "/auth/callback?code=unused&state=expected",
        &flow,
        vec![],
    )
    .await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn login_intake_is_bounded_and_concurrent_callbacks_consume_state_once() {
    let db = fixture().await;
    let other = reopen(&db).await;
    let flow = Pending {
        state: "state".into(),
        verifier: "verifier".into(),
        invitation: None,
        expires: now() + 600,
    };
    let hash = identity::digest("one");
    db.save_login_flow(&hash, &flow).await.unwrap();
    let (a, b) = tokio::join!(db.take_login_flow(&hash), other.take_login_flow(&hash));
    assert_eq!(
        usize::from(a.unwrap().is_some()) + usize::from(b.unwrap().is_some()),
        1
    );
    for i in 0..100 {
        db.save_login_flow(&identity::digest(&i.to_string()), &flow)
            .await
            .unwrap();
    }
    assert!(other.save_login_flow(&hash, &flow).await.is_err());
    execute(&db.db, "UPDATE relay_login_flows SET expires=0", vec![])
        .await
        .unwrap();
    other.save_login_flow(&hash, &flow).await.unwrap();
    let n: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_login_flows",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn hosted_workspace_does_not_create_local_runtime_and_media_survives_api_replacement() {
    let db = fixture().await;
    let owner = db
        .login(
            "user_hosted",
            "owner@example.test",
            &identity::digest("token"),
            "session_hosted",
        )
        .await
        .unwrap();
    let w = workspace(&db, &owner).await;
    let other = user(&db, "other").await;
    let denied = workspace(&db, &other).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let agent = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Nova".into(),
        role: "Assistant".into(),
        model: "fixture".into(),
        scope: "workspace".into(),
        revision: 0,
        settings: Settings::default(),
    };
    db.save_agent_profile(&owner, &w, &agent).await.unwrap();
    let (provider, _, server) = provider().await;
    let root = tempfile::tempdir().unwrap();
    // Exercise the S3 branch with a shared object-store fixture, not a local disk fallback.
    let storage = identity::library::Storage::S3(Arc::new(object_store::memory::InMemory::new()));
    let first = app(db, root.path(), &provider, storage.clone());
    let cookie = "ayati_identity=token";
    let (status, _, bytes) = call(
        &first,
        "GET",
        &format!("/api/identity/workspaces/{w}/shared-agents"),
        cookie,
        vec![],
    )
    .await;
    assert_eq!(status, 200);
    assert!(String::from_utf8(bytes).unwrap().contains(&agent.id));
    for path in ["agents", "apps", "panels", "home"] {
        assert_eq!(
            call(&first, "GET", &format!("/w/{w}/api/{path}"), cookie, vec![])
                .await
                .0,
            200
        );
    }
    assert_eq!(
        call(
            &first,
            "POST",
            &format!("/w/{w}/api/vm/start"),
            cookie,
            b"{}".to_vec()
        )
        .await
        .0,
        503
    );
    assert_eq!(
        call(
            &first,
            "GET",
            &format!("/w/{denied}/api/agents"),
            cookie,
            vec![]
        )
        .await
        .0,
        403
    );
    assert!(!root.path().join("workspaces").exists());
    let bytes = b"durable hosted attachment";
    let (status, _, object) = call(
        &first,
        "POST",
        &format!("/api/identity/workspaces/{w}/chats/{c}/objects?name=note.txt"),
        cookie,
        bytes.to_vec(),
    )
    .await;
    assert_eq!(status, 200);
    let id = serde_json::from_slice::<Value>(&object).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fresh = tempfile::tempdir().unwrap();
    let second = app(reopen(&first.store).await, fresh.path(), &provider, storage);
    drop(first);
    let download = format!("/api/identity/workspaces/{w}/objects/{id}");
    let (status, _, saved) = call(&second, "GET", &download, cookie, vec![]).await;
    assert_eq!(status, 200);
    assert_eq!(saved, bytes);
    assert_ne!(
        call(
            &second,
            "GET",
            &format!("/api/identity/workspaces/{denied}/objects/{id}"),
            cookie,
            vec![]
        )
        .await
        .0,
        200
    );
    assert!(!fresh.path().join("workspaces").exists());
    // Liveness stays up while readiness fails after losing the database pool.
    second.store.db.clone().close().await.unwrap();
    assert_eq!(
        identity::hosting::ready(axum::extract::State(second.clone())).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(identity::hosting::live().await, StatusCode::OK);
    server.abort();
}
