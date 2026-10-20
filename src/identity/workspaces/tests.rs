use super::*;
use crate::platform::home_preferences::HomePreferences;

fn setup(root: &std::path::Path) -> (Shared, User, User, String, String) {
    let store = Store::open(&root.join("identity.sqlite")).unwrap();
    let owner = store
        .login("owner", "owner@example.com", "owner_hash", "owner_session")
        .unwrap();
    let other = store
        .login("other", "other@example.com", "other_hash", "other_session")
        .unwrap();
    let first = uuid::Uuid::new_v4().to_string();
    let second = uuid::Uuid::new_v4().to_string();
    store.create(&owner, "First", &first).unwrap();
    store.create(&other, "Second", &second).unwrap();
    let app = Arc::new(App {
        provider: Provider::test_provider(),
        store: store.into(),
        origin: "http://127.0.0.1:8765".into(),
        pending: Mutex::new(HashMap::new()),
        runtimes: Runtimes::new(root.join("workspaces")),
    });
    (app, owner, other, first, second)
}
fn request(method: Method, path: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:8765")
        .body(Body::empty())
        .unwrap()
}
#[tokio::test]
async fn forwarded_resource_routes_do_not_inherit_outer_path_parameters() {
    let temp = tempfile::tempdir().unwrap();
    let (app, owner, _, first, _) = setup(temp.path());
    let gateway = axum::Router::new().route(
        "/w/{workspace}/api/{*rest}",
        axum::routing::get({
            let app = app.clone();
            move |RoutePath((workspace, rest)): RoutePath<(String, String)>,
                  request: Request<Body>| {
                dispatch_for(app.clone(), owner.clone(), workspace, rest, request)
            }
        }),
    );
    for route in ["apps/missing/grants", "apps/missing/document/missing"] {
        let response = gateway
            .clone()
            .oneshot(request(Method::GET, &format!("/w/{first}/api/{route}")))
            .await
            .unwrap();
        // Reach the business handler rather than failing Axum's Path extractor.
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        if route.ends_with("grants") {
            assert_eq!(value["grants"], json!([]));
        } else {
            assert!(value.get("error").is_some());
        }
    }
    app.runtimes.shutdown().await.unwrap();
}
#[tokio::test]
async fn cross_workspace_and_member_requests_cannot_inherit_owner_authority() {
    let temp = tempfile::tempdir().unwrap();
    let (app, owner, other, first, _) = setup(temp.path());
    let denied = dispatch_for(
        app.clone(),
        other.clone(),
        first.clone(),
        "home".into(),
        request(Method::GET, "/"),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert!(app.runtimes.states.lock().await.is_empty());
    let invite = app
        .store
        .sqlite()
        .reserve_invite(&owner, &first, &other.email, "Member")
        .unwrap();
    app.store.sqlite().delivered(&invite, Some("sent")).unwrap();
    app.store.sqlite().accept(&other, &invite).unwrap();
    for route in [
        "messages",
        "permissions/foreign",
        "apps/foreign/session",
        "agents",
        "shutdown",
    ] {
        assert_eq!(
            dispatch_for(
                app.clone(),
                other.clone(),
                first.clone(),
                route.into(),
                request(Method::POST, "/")
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
    }
    for route in [
        "files/foreign",
        "events",
        "state",
        "apps/foreign/document/foreign",
    ] {
        assert_eq!(
            dispatch_for(
                app.clone(),
                other.clone(),
                first.clone(),
                route.into(),
                request(Method::GET, "/")
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
    }
    let response = dispatch_for(
        app.clone(),
        other.clone(),
        first.clone(),
        "home".into(),
        request(Method::GET, "/"),
    )
    .await;
    let bytes = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        json!({"agents":[]})
    );
    app.store
        .sqlite()
        .change(&owner, &first, &other.id, None)
        .unwrap();
    assert_eq!(
        dispatch_for(
            app.clone(),
            other,
            first,
            "home".into(),
            request(Method::GET, "/")
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}
#[tokio::test]
async fn runtimes_have_independent_storage_and_never_reuse_legacy_vm_or_connectors() {
    let temp = tempfile::tempdir().unwrap();
    let (app, owner, _, first, second) = setup(temp.path());
    let a = app.runtimes.get(&first, 8765).await.unwrap();
    let b = app.runtimes.get(&second, 8765).await.unwrap();
    assert!(!Arc::ptr_eq(&a.agent, &b.agent));
    assert!(Arc::ptr_eq(
        &a.agent,
        &app.runtimes.get(&first, 8765).await.unwrap().agent
    ));
    let pa = a.agent.platform().unwrap();
    let pb = b.agent.platform().unwrap();
    assert_ne!(pa.user_id, pb.user_id);
    assert!(pa.client.is_none() && pb.client.is_none());
    pa.save_home_preferences(&HomePreferences {
        location: "First only".into(),
        timezone: "Asia/Kolkata".into(),
        hour12: true,
    })
    .unwrap();
    assert!(pb.home_preferences().unwrap().is_none());
    assert!(!a.model_ready && !b.model_ready);
    assert!(
        !a.workspace
            .as_ref()
            .unwrap()
            .agent("local-agent")
            .unwrap()
            .2
    );
    let response = dispatch_for(
        app.clone(),
        owner,
        first.clone(),
        "home/preferences".into(),
        request(Method::GET, "/"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["preferences"]["location"],
        "First only"
    );
    app.runtimes.shutdown().await.unwrap();
    let reopened = crate::platform::Platform::open(
        &temp.path().join("workspaces").join(first).join("business"),
        None,
    )
    .unwrap();
    assert_eq!(
        reopened.home_preferences().unwrap().unwrap().location,
        "First only"
    );
}
#[tokio::test]
async fn workspace_gateway_requires_session_and_same_origin_mutations() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _, _, first, _) = setup(temp.path());
    let uri = format!("/w/{first}/api/home");
    assert_eq!(
        router(app.clone())
            .oneshot(request(Method::GET, &uri))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        router(app.clone())
            .oneshot(request(Method::POST, &uri))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router(app)
            .oneshot(request(Method::GET, "/api/home"))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}
#[tokio::test]
async fn logout_closes_an_existing_event_stream() {
    let temp = tempfile::tempdir().unwrap();
    let (app, owner, _, first, _) = setup(temp.path());
    app.store
        .sqlite()
        .login("owner", "owner@example.com", &digest("token"), "session")
        .unwrap();
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::COOKIE, "ayati_identity=token".parse().unwrap());
    let body = Body::from_stream(futures_util::stream::pending::<
        Result<axum::body::Bytes, std::io::Error>,
    >());
    let response = guarded_stream(app.clone(), owner, first, headers, Response::new(body));
    app.store.sqlite().logout(&digest("token")).unwrap();
    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        axum::body::to_bytes(response.into_body(), 100),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(bytes.is_empty());
}

#[tokio::test]
async fn root_preserves_explicit_workspace_chooser() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _, _, _, _) = setup(temp.path());
    let response = router(app)
        .oneshot(request(Method::GET, "/?view=account&choose=1"))
        .await
        .unwrap();
    assert_eq!(
        response.headers()[header::LOCATION],
        "/index.html?view=account&choose=1"
    );
}

#[tokio::test]
async fn startup_snapshot_marks_only_the_bound_owner_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let (mut app, owner, other, first, second) = setup(temp.path());
    Arc::get_mut(&mut app).unwrap().runtimes.bound = Some((first.clone(), owner.id.clone()));
    let snapshot = session_snapshot(&app, &owner).await.unwrap();
    assert_eq!(snapshot["workspaces"][0]["id"], first);
    assert_eq!(snapshot["workspaces"][0]["adopted"], true);
    let other_snapshot = session_snapshot(&app, &other).await.unwrap();
    assert_eq!(other_snapshot["workspaces"][0]["id"], second);
    assert_eq!(other_snapshot["workspaces"][0]["adopted"], false);
    // Installation ownership alone must not survive a lost Owner membership.
    app.store
        .sqlite()
        .0
        .lock()
        .unwrap()
        .execute(
            "UPDATE members SET role='Member' WHERE workspace=?1 AND user=?2",
            [&first, &owner.id],
        )
        .unwrap();
    assert_eq!(
        session_snapshot(&app, &owner).await.unwrap()["workspaces"][0]["adopted"],
        false
    );
}
