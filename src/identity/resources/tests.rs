use super::*;
use rusqlite::params;
fn setup(root: &Path) -> (Shared, User, User, User, String) {
    let store = Store::open(&root.join("identity.sqlite")).unwrap();
    let owner = store.login("owner", "owner@example.com", "a", "s").unwrap();
    let member = store
        .login("member", "member@example.com", "b", "s")
        .unwrap();
    let second = store
        .login("second", "second@example.com", "c", "s")
        .unwrap();
    let w = uuid::Uuid::new_v4().to_string();
    store.create(&owner, "Business", &w).unwrap();
    for user in [&member, &second] {
        let invite = store
            .reserve_invite(&owner, &w, &user.email, "User")
            .unwrap();
        store.delivered(&invite, Some("sent")).unwrap();
        store.accept(user, &invite).unwrap();
    }
    let app = Arc::new(App {
        provider: Provider::test_provider(),
        store: store.into(),
        origin: "http://127.0.0.1:8765".into(),
        pending: Mutex::new(HashMap::new()),
        simulation: None,
        runtimes: workspaces::Runtimes::new(root.join("workspaces")),
    });
    (app, owner, member, second, w)
}
fn request(method: Method, body: Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method);
    if let Some(token) = token {
        builder = builder.header("x-ayati-app-session", token);
    }
    builder
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}
async fn value(response: Response) -> Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 100000)
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn fixture(app: &Shared, root: &Path, w: &str) -> String {
    platform(app, w).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let db = rusqlite::Connection::open(
        root.join("workspaces")
            .join(w)
            .join("business/platform.sqlite"),
    )
    .unwrap();
    db.execute(
        "INSERT INTO apps VALUES(?1,'local-agent',2,1,'saved')",
        [&id],
    )
    .unwrap();
    for (revision, name) in [(1, "Published inventory"), (2, "PRIVATE DRAFT")] {
        let definition = json!({"id":id,"expected_revision":0,"name":name,"description":"Inventory","html":"<p>App</p>","css":"","javascript":"","collections":{"items":{"name":"string"}}});
        db.execute("INSERT INTO app_versions(app_id,revision,definition,task_id) VALUES(?1,?2,?3,'fixture')",params![id,revision,definition.to_string()]).unwrap();
    }
    id
}
fn give(app: &App, w: &str, user: &User, id: &str, access: &str) {
    app.store
        .sqlite()
        .0
        .lock()
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO resource_grants VALUES(?1,?2,'app',?3,?4)",
            params![w, user.id, id, access],
        )
        .unwrap();
}
#[tokio::test]
async fn members_only_see_granted_published_apps_and_cannot_open_previews() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _, member, _, w) = setup(temp.path());
    let id = fixture(&app, temp.path(), &w).await;
    let empty = value(
        member_inner(
            &app,
            &member,
            &w,
            "/api/apps",
            request(Method::GET, Value::Null, None),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(empty["apps"].as_array().unwrap().len(), 0);
    give(&app, &w, &member, &id, "read");
    let listing = value(
        member_inner(
            &app,
            &member,
            &w,
            "/api/apps",
            request(Method::GET, Value::Null, None),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(listing["apps"][0]["name"], "Published inventory");
    assert!(!listing.to_string().contains("PRIVATE DRAFT"));
    assert!(
        member_inner(
            &app,
            &member,
            &w,
            &format!("/api/apps/{id}/session"),
            request(Method::POST, json!({"revision":2,"preview":true}), None)
        )
        .await
        .is_err()
    );
    assert!(
        member_inner(
            &app,
            &member,
            &w,
            &format!("/api/apps/{id}/archive"),
            request(Method::POST, json!({"revision":2}), None)
        )
        .await
        .is_err()
    );
    app.runtimes.shutdown().await.unwrap();
}
#[tokio::test]
async fn read_only_and_session_ownership_are_enforced_and_revocation_blocks_reuse() {
    let temp = tempfile::tempdir().unwrap();
    let (app, owner, member, other, w) = setup(temp.path());
    let id = fixture(&app, temp.path(), &w).await;
    give(&app, &w, &member, &id, "read");
    give(&app, &w, &other, &id, "write");
    let session = value(
        member_inner(
            &app,
            &member,
            &w,
            &format!("/api/apps/{id}/session"),
            request(Method::POST, json!({"revision":1,"preview":false}), None),
        )
        .await
        .unwrap(),
    )
    .await;
    let token = session["session"].as_str().unwrap();
    let document = session["document_id"].as_str().unwrap();
    let list = json!({"action":"list","collection":"items","after":null});
    let path = format!("/api/apps/{id}/records");
    assert!(
        member_inner(
            &app,
            &member,
            &w,
            &path,
            request(Method::POST, list.clone(), Some(token))
        )
        .await
        .is_ok()
    );
    assert!(
        member_inner(
            &app,
            &other,
            &w,
            &path,
            request(Method::POST, list.clone(), Some(token))
        )
        .await
        .is_err()
    );
    assert!(
        member_inner(
            &app,
            &other,
            &w,
            &format!("/api/apps/{id}/document/{document}"),
            request(Method::GET, Value::Null, None)
        )
        .await
        .is_err()
    );
    let save = json!({"action":"save","collection":"items","id":null,"expected_revision":0,"values":{"name":"Test"},"request_id":uuid::Uuid::new_v4().to_string()});
    assert!(
        member_inner(
            &app,
            &member,
            &w,
            &path,
            request(Method::POST, save.clone(), Some(token))
        )
        .await
        .is_err()
    );
    give(&app, &w, &member, &id, "write");
    assert!(
        member_inner(
            &app,
            &member,
            &w,
            &path,
            request(Method::POST, save, Some(token))
        )
        .await
        .is_ok()
    );
    app.store
        .sqlite()
        .change(&owner, &w, &member.id, None)
        .unwrap();
    assert!(
        member_inner(
            &app,
            &member,
            &w,
            &path,
            request(Method::POST, list, Some(token))
        )
        .await
        .is_err()
    );
    app.runtimes.shutdown().await.unwrap();
}
