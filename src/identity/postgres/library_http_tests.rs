use super::tests::*;
use crate::identity::{self, library::model::checksum};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::json;
use tower::ServiceExt;
const ORIGIN: &str = "http://127.0.0.1:9897";
async fn call(
    router: &Router,
    method: &str,
    path: &str,
    token: &str,
    body: Vec<u8>,
    extra: Option<(&str, &str)>,
) -> (u16, axum::http::HeaderMap, Vec<u8>) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("Host", "127.0.0.1:9897")
        .header("Cookie", format!("ayati_identity={token}"))
        .header("Origin", ORIGIN)
        .header("X-Ayati-Request", "identity")
        .header("Content-Type", "application/json");
    if let Some((key, value)) = extra {
        request = request.header(key, value);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 32 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn library_http_upload_download_ranges_revocation_and_mutation_guards() {
    let db = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let owner = db
        .login(
            "simulation_employee_owner",
            "owner@example.test",
            &identity::digest("owner-token"),
            "local-simulation",
        )
        .await
        .unwrap();
    let member = db
        .login(
            "simulation_employee_member",
            "member@example.test",
            &identity::digest("member-token"),
            "local-simulation",
        )
        .await
        .unwrap();
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "User").await;
    let app = std::sync::Arc::new(identity::App {
        library: identity::library::Storage::Local(root.path().join("objects")),
        provider: identity::Provider::simulation(),
        store: identity::database::Database::Postgres(db),
        origin: ORIGIN.into(),
        pending: Default::default(),
        runtimes: identity::workspaces::Runtimes::new(root.path().join("workspaces")),
        simulation: Some(identity::simulation::Simulation {
            employees: vec![],
            workspace: w.clone(),
            root: root.path().into(),
            paused: std::sync::atomic::AtomicBool::new(true),
        }),
    });
    let router = identity::router(app);
    let bytes = b"A reference file. Confirm delivery with the owner.";
    let id = uuid::Uuid::new_v4().to_string();
    let upload = json!({"id":id,"request":uuid::Uuid::new_v4().to_string(),"title":"reference.txt","size":bytes.len(),"checksum":checksum(bytes)});
    let base = format!("/api/identity/workspaces/{w}");
    assert_eq!(
        call(
            &router,
            "POST",
            &format!("{base}/library-uploads"),
            "owner-token",
            upload.to_string().into_bytes(),
            None
        )
        .await
        .0,
        200
    );
    let endpoint = format!("{base}/library-uploads/{id}/bytes");
    assert_eq!(
        call(
            &router,
            "POST",
            &endpoint,
            "owner-token",
            b"Wrong file".to_vec(),
            None
        )
        .await
        .0,
        400
    );
    let saved = call(
        &router,
        "POST",
        &endpoint,
        "owner-token",
        bytes.to_vec(),
        Some(("Content-Type", "application/octet-stream")),
    )
    .await;
    assert_eq!(saved.0, 200, "{}", String::from_utf8_lossy(&saved.2));
    assert_eq!(
        call(
            &router,
            "POST",
            &endpoint,
            "owner-token",
            bytes.to_vec(),
            None
        )
        .await
        .0,
        200
    );
    let download = format!("{base}/library/{id}/download");
    let denied = call(&router, "GET", &download, "member-token", vec![], None).await;
    assert_eq!(denied.0, 404);
    let complete = call(&router, "GET", &download, "owner-token", vec![], None).await;
    assert_eq!(complete.0, 200);
    assert_eq!(complete.2, bytes);
    assert_eq!(complete.1["cache-control"], "no-store");
    let range = call(
        &router,
        "GET",
        &download,
        "owner-token",
        vec![],
        Some(("Range", "bytes=0-10")),
    )
    .await;
    assert_eq!(range.0, 206);
    assert_eq!(range.2, &bytes[..11]);
    assert_eq!(
        call(&router, "HEAD", &download, "owner-token", vec![], None)
            .await
            .0,
        200
    );
    assert_eq!(
        call(
            &router,
            "GET",
            &download,
            "owner-token",
            vec![],
            Some(("Range", "bytes=999-1000"))
        )
        .await
        .0,
        416
    );
    let sharing = json!({"request":uuid::Uuid::new_v4().to_string(),"revision":1,"visibility":"selected","people":[{"user":member.id,"access":"view"}],"agents":[]});
    assert_eq!(
        call(
            &router,
            "POST",
            &format!("{base}/library/{id}/share"),
            "owner-token",
            sharing.to_string().into_bytes(),
            None
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(&router, "GET", &download, "member-token", vec![], None)
            .await
            .0,
        200
    );
    let revoke = json!({"request":uuid::Uuid::new_v4().to_string(),"revision":2,"visibility":"private","people":[],"agents":[]});
    assert_eq!(
        call(
            &router,
            "POST",
            &format!("{base}/library/{id}/share"),
            "owner-token",
            revoke.to_string().into_bytes(),
            None
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(&router, "HEAD", &download, "member-token", vec![], None)
            .await
            .0,
        404
    );
    let unguarded = Request::builder()
        .method("POST")
        .uri(format!("{base}/library/{id}/share"))
        .header("Host", "127.0.0.1:9897")
        .header("Cookie", "ayati_identity=owner-token")
        .body(Body::from(revoke.to_string()))
        .unwrap();
    assert_eq!(router.oneshot(unguarded).await.unwrap().status(), 403);
}
