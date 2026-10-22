use super::*;
use axum::http::Request;
use tower::ServiceExt;

#[test]
fn membership_invitation_and_revocation_are_transactional() {
    let db = Store::open(Path::new(":memory:")).unwrap();
    let owner = db
        .login("user_owner", "owner@example.com", "a", "session_a")
        .unwrap();
    let employee = db
        .login("user_employee", "employee@example.com", "b", "session_b")
        .unwrap();
    let outsider = db
        .login("user_outsider", "outsider@example.com", "c", "session_c")
        .unwrap();
    let workspace = uuid::Uuid::new_v4().to_string();
    db.create(&owner, "Business", &workspace).unwrap();
    assert!(db.people(&outsider, &workspace).is_err());
    let invitation = db
        .reserve_invite(&owner, &workspace, &employee.email, "User")
        .unwrap();
    assert!(db.accept(&employee, &invitation).is_err()); // No confirmed provider delivery.
    db.delivered(&invitation, Some("invitation_test")).unwrap();
    assert!(db.accept(&outsider, &invitation).is_err());
    db.accept(&employee, &invitation).unwrap();
    assert!(
        db.reserve_invite(&employee, &workspace, "someone@example.com", "User")
            .is_err()
    );
    assert!(db.change(&owner, &workspace, &owner.id, None).is_err());
    db.change(&owner, &workspace, &employee.id, None).unwrap();
    assert!(db.people(&employee, &workspace).is_err());
    assert!(db.accept(&employee, &invitation).is_err());
}

#[test]
fn cancelled_expired_and_demoted_invites_cannot_grant_access() {
    let db = Store::open(Path::new(":memory:")).unwrap();
    let owner = db
        .login("user_owner", "owner@example.com", "a", "session_a")
        .unwrap();
    let admin = db
        .login("user_admin", "admin@example.com", "b", "session_b")
        .unwrap();
    let employee = db
        .login("user_employee", "employee@example.com", "c", "session_c")
        .unwrap();
    let workspace = uuid::Uuid::new_v4().to_string();
    db.create(&owner, "Business", &workspace).unwrap();
    let admin_invite = db
        .reserve_invite(&owner, &workspace, &admin.email, "User")
        .unwrap();
    db.delivered(&admin_invite, Some("invitation_admin"))
        .unwrap();
    db.accept(&admin, &admin_invite).unwrap();
    db.change(&owner, &workspace, &admin.id, Some("Owner"))
        .unwrap();
    assert!(
        db.reserve_invite(&admin, &workspace, "another@example.com", "Admin")
            .is_err()
    );
    let invitation = db
        .reserve_invite(&admin, &workspace, &employee.email, "User")
        .unwrap();
    db.delivered(&invitation, Some("invitation_employee"))
        .unwrap();
    db.change(&owner, &workspace, &admin.id, Some("User"))
        .unwrap();
    assert!(db.accept(&employee, &invitation).is_err());
    db.cancel(&owner, &workspace, &invitation).unwrap();
    assert!(db.accept(&employee, &invitation).is_err());
    let invitation = db
        .reserve_invite(&owner, &workspace, &employee.email, "User")
        .unwrap();
    db.delivered(&invitation, Some("invitation_new")).unwrap();
    db.0.lock()
        .unwrap()
        .execute(
            "UPDATE invitations SET expires=0 WHERE id=?1",
            [&invitation],
        )
        .unwrap();
    assert!(db.accept(&employee, &invitation).is_err());
}

#[test]
fn sessions_expire_and_logout_removes_access() {
    let db = Store::open(Path::new(":memory:")).unwrap();
    db.login("user_owner", "owner@example.com", "hash", "session_a")
        .unwrap();
    assert!(db.session("hash").unwrap().is_some());
    assert!(db.session("forged").unwrap().is_none());
    db.logout("hash").unwrap();
    assert!(db.session("hash").unwrap().is_none());
    db.login("user_owner", "owner@example.com", "expired", "session_b")
        .unwrap();
    db.0.lock()
        .unwrap()
        .execute("UPDATE sessions SET expires=0 WHERE hash='expired'", [])
        .unwrap();
    assert!(db.session("expired").unwrap().is_none());
}

#[tokio::test]
async fn guards_reject_cross_origin_posts_legacy_routes_and_invalid_callback() {
    let app = Arc::new(App {
        library: Default::default(),
        provider: Provider::test_provider(),
        store: Store::open(Path::new(":memory:")).unwrap().into(),
        origin: "http://127.0.0.1:8765".into(),
        pending: Mutex::new(HashMap::new()),
        simulation: None,
        runtimes: workspaces::Runtimes::new(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
        ),
    });
    for (path, method, host, expected) in [
        (
            "/api/identity/logout",
            "POST",
            "127.0.0.1:8765",
            StatusCode::FORBIDDEN,
        ),
        (
            "/api/identity/session",
            "GET",
            "evil.example",
            StatusCode::FORBIDDEN,
        ),
        ("/api/state", "GET", "127.0.0.1:8765", StatusCode::NOT_FOUND),
        (
            "/api/identity/simulation",
            "GET",
            "127.0.0.1:8765",
            StatusCode::NOT_FOUND,
        ),
    ] {
        let response = router(app.clone())
            .oneshot(
                Request::builder()
                    .uri(path)
                    .method(method)
                    .header("host", host)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .uri("/auth/callback?code=fake&state=fake")
                .header("host", "127.0.0.1:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers()["location"]
            .to_str()
            .unwrap()
            .contains("login_error")
    );
    assert!(app.store.sqlite().session("fake").unwrap().is_none());
}

#[tokio::test]
async fn login_is_pkce_bound_and_callback_state_is_single_use() {
    let app = Arc::new(App {
        library: Default::default(),
        provider: Provider::test_provider(),
        store: Store::open(Path::new(":memory:")).unwrap().into(),
        origin: "http://127.0.0.1:8765".into(),
        pending: Mutex::new(HashMap::new()),
        simulation: None,
        runtimes: workspaces::Runtimes::new(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
        ),
    });
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .header("host", "127.0.0.1:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
    let pair = cookie.split(';').next().unwrap();
    let token = pair.split_once('=').unwrap().1;
    let url = reqwest::Url::parse(response.headers()[header::LOCATION].to_str().unwrap()).unwrap();
    let params = url.query_pairs().collect::<HashMap<_, _>>();
    assert_eq!(params["code_challenge_method"], "S256");
    {
        let map = app.pending.lock().unwrap();
        let pending = map.get(&digest(token)).unwrap();
        assert_eq!(params["code_challenge"], digest(&pending.verifier));
        assert_eq!(params["state"], pending.state);
    }
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .uri("/auth/callback?code=unused&state=wrong")
                .header("host", "127.0.0.1:8765")
                .header("cookie", pair)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        response.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .contains("login_error")
    );
    assert!(app.pending.lock().unwrap().is_empty());
}

#[test]
fn duplicate_workspace_request_is_safe_and_cross_owner_reuse_is_denied() {
    let db = Store::open(Path::new(":memory:")).unwrap();
    let owner = db
        .login("user_owner", "owner@example.com", "a", "session_a")
        .unwrap();
    let other = db
        .login("user_other", "other@example.com", "b", "session_b")
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    db.create(&owner, "Business", &id).unwrap();
    db.create(&owner, "Business", &id).unwrap();
    assert_eq!(
        db.snapshot(&owner).unwrap()["workspaces"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(db.create(&other, "Business", &id).is_err());
    assert!(db.create(&owner, "Different", &id).is_err());
}

#[tokio::test]
async fn postgres_errors_do_not_expose_database_values() {
    let error = Error::from(sea_orm::DbErr::Custom("private row value".into()));
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let bytes = axum::body::to_bytes(response.into_body(), 10000)
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("private row value"));
}

#[tokio::test]
async fn only_versioned_static_code_can_be_cached() {
    use axum::http::Method;
    for path in [
        "/assets/index-Abcd1234.js",
        "/assets/home-aBc_12-4.css",
        "/assets/pdf.worker.min-CjEcRF4W.mjs",
    ] {
        assert!(cacheable_asset(&Method::GET, path));
        assert!(cacheable_asset(&Method::HEAD, path));
        assert!(!cacheable_asset(&Method::POST, path));
    }
    for path in [
        "/index.html",
        "/assets/index.html",
        "/assets/app.js",
        "/assets/pdf.worker.mjs",
        "/assets/file-Abcd1234.json",
        "/assets/nested/file-Abcd1234.js",
        "/api/identity/session",
        "/w/tenant/api/apps/file-Abcd1234.js",
    ] {
        assert!(!cacheable_asset(&Method::GET, path));
    }
    let app = Arc::new(App {
        library: Default::default(),
        provider: Provider::test_provider(),
        store: Store::open(Path::new(":memory:")).unwrap().into(),
        origin: "http://127.0.0.1:8765".into(),
        pending: Mutex::new(HashMap::new()),
        simulation: None,
        runtimes: workspaces::Runtimes::new(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
        ),
    });
    let routes = Router::new()
        .route("/assets/entry-Abcd1234.js", get(|| async { "export {}" }))
        .route("/index.html", get(|| async { "<html></html>" }))
        .route(
            "/api/identity/session",
            get(|| async { "private identity" }),
        )
        .layer(middleware::from_fn_with_state(app, guard));
    for (path, expected) in [
        (
            "/assets/entry-Abcd1234.js",
            "private, max-age=31536000, immutable",
        ),
        ("/assets/missing-Abcd1234.js", "no-store"),
        ("/api/identity/session", "no-store"),
        ("/index.html", "no-store"),
    ] {
        let response = routes
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("host", "127.0.0.1:8765")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.headers()[header::CACHE_CONTROL], expected);
        assert!(response.headers().contains_key("content-security-policy"));
    }
}
