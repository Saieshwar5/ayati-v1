use super::*;
use tower::ServiceExt;

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
        library: Default::default(),
        provider: Provider::test_provider(),
        store: store.into(),
        origin: "http://127.0.0.1:8765".into(),
        pending: Mutex::new(HashMap::new()),
        simulation: None,
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
async fn body(response: Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
#[tokio::test]
async fn unported_workspace_routes_answer_like_the_hosted_gateway() {
    let temp = tempfile::tempdir().unwrap();
    let (app, owner, _, first, _) = setup(temp.path());
    let call = |rest: &str, method: Method| {
        dispatch_for(
            app.clone(),
            owner.clone(),
            first.clone(),
            rest.into(),
            request(method, "/"),
        )
    };
    let context = body(call("context", Method::GET).await).await;
    assert_eq!(context["role"], "Owner");
    assert!(context.get("resource_access").is_none());
    assert_eq!(
        body(call("apps", Method::GET).await).await,
        json!({"apps":[],"available":false})
    );
    assert_eq!(
        body(call("home/preferences", Method::GET).await).await,
        json!({"preferences":null})
    );
    for (rest, method) in [
        ("home", Method::POST),
        ("events", Method::GET),
        ("apps/any/document/any", Method::GET),
        ("shutdown", Method::POST),
    ] {
        assert_eq!(
            call(rest, method).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
    assert!(!temp.path().join("workspaces").join(&first).exists());
}
#[tokio::test]
async fn workspace_routes_require_current_membership() {
    let temp = tempfile::tempdir().unwrap();
    let (app, owner, other, first, _) = setup(temp.path());
    let call = |user: User| {
        dispatch_for(
            app.clone(),
            user,
            first.clone(),
            "home".into(),
            request(Method::GET, "/"),
        )
    };
    assert_eq!(call(other.clone()).await.status(), StatusCode::FORBIDDEN);
    let invite = app
        .store
        .sqlite()
        .reserve_invite(&owner, &first, &other.email, "User")
        .unwrap();
    app.store.sqlite().delivered(&invite, Some("sent")).unwrap();
    app.store.sqlite().accept(&other, &invite).unwrap();
    assert_eq!(body(call(other.clone()).await).await, json!({"agents":[]}));
    app.store
        .sqlite()
        .change(&owner, &first, &other.id, None)
        .unwrap();
    assert_eq!(call(other).await.status(), StatusCode::FORBIDDEN);
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
