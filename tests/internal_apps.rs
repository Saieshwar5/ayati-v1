use ayati::{
    platform::{ExecutionRun, Platform, app_records::RecordCommand, apps::AppDraft},
    storage::Store,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

fn draft() -> AppDraft {
    serde_json::from_value(json!({"id":uuid::Uuid::new_v4().to_string(),"expected_revision":0,"name":"Inventory","description":"Synthetic internal app","html":"<h1>Inventory</h1>","css":"body{font-family:sans-serif}","javascript":"document.body.dataset.ready='yes'","collections":{"items":{"name":"string","quantity":"number"}}})).unwrap()
}
fn command(value: Value) -> RecordCommand {
    serde_json::from_value(value).unwrap()
}
fn list() -> RecordCommand {
    command(json!({"action":"list","collection":"items"}))
}
fn save(name: &str) -> RecordCommand {
    command(
        json!({"action":"save","collection":"items","expected_revision":0,"values":{"name":name,"quantity":2},"request_id":uuid::Uuid::new_v4().to_string()}),
    )
}

#[tokio::test]
async fn previews_are_separate_and_record_receipts_revisions_and_deletes_are_durable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("business");
    let p = Platform::open(&path, None).unwrap();
    let s = Store::open(&dir.path().join("builder.sqlite")).unwrap();
    s.enqueue("Build inventory").unwrap();
    let claim = s.claim_next("worker").unwrap().unwrap();
    let cancel = CancellationToken::new();
    let app = draft();
    p.write_app(
        ExecutionRun {
            agent: "builder",
            store: &s,
            claim: &claim,
            cancel: &cancel,
        },
        &app,
    )
    .await
    .unwrap();
    assert!(
        p.apps(Some("foreign"))
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        p.activate_app(&app.id, 1, "not-reviewed", None)
            .await
            .is_err()
    );
    assert!(p.app_session(&app.id, 1, false).await.is_err());
    let preview = p.app_session(&app.id, 1, true).await.unwrap();
    let token = preview["session"].as_str().unwrap();
    let first = save("Preview only");
    let saved = p.session_records(&app.id, token, &first).await.unwrap();
    assert_eq!(
        p.session_records(&app.id, token, &first).await.unwrap(),
        saved
    );
    assert_eq!(
        p.session_records(&app.id, token, &list()).await.unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let document = p
        .app_document(&app.id, preview["document_id"].as_str().unwrap())
        .unwrap();
    assert!(!document.contains(token));
    p.activate_app(&app.id, 1, token, None).await.unwrap();
    let live = p.app_session(&app.id, 1, false).await.unwrap();
    let live_token = live["session"].as_str().unwrap();
    assert!(
        p.session_records(&app.id, live_token, &list())
            .await
            .unwrap()["records"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        p.session_records(&app.id, preview["document_id"].as_str().unwrap(), &list())
            .await
            .is_err()
    );
    let live_record = p
        .session_records(&app.id, live_token, &save("Live item"))
        .await
        .unwrap();
    let stale = command(
        json!({"action":"save","collection":"items","id":live_record["id"],"expected_revision":0,"values":{"quantity":3},"request_id":uuid::Uuid::new_v4().to_string()}),
    );
    assert!(
        p.session_records(&app.id, live_token, &stale)
            .await
            .is_err()
    );
    let wrong = command(
        json!({"action":"save","collection":"items","id":live_record["id"],"expected_revision":1,"values":{"quantity":"wrong type"},"request_id":uuid::Uuid::new_v4().to_string()}),
    );
    assert!(
        p.session_records(&app.id, live_token, &wrong)
            .await
            .is_err()
    );
    let delete = command(
        json!({"action":"delete","collection":"items","id":live_record["id"],"expected_revision":1,"request_id":uuid::Uuid::new_v4().to_string()}),
    );
    p.session_records(&app.id, live_token, &delete)
        .await
        .unwrap();
    p.session_records(&app.id, live_token, &delete)
        .await
        .unwrap();
    assert!(
        p.session_records(&app.id, live_token, &list())
            .await
            .unwrap()["records"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    drop(p);
    let p = Platform::open(&path, None).unwrap();
    assert!(
        p.session_records(&app.id, live_token, &list())
            .await
            .unwrap()["records"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let db = rusqlite::Connection::open(path.join("platform.sqlite")).unwrap();
    let retained: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM app_record_history WHERE dataset='live'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, 2);
}

#[tokio::test]
async fn exact_release_activation_preserves_live_records_and_code_rollback_retains_added_fields() {
    let dir = tempfile::tempdir().unwrap();
    let p = Platform::open(&dir.path().join("business"), None).unwrap();
    let s = Store::open(&dir.path().join("builder.sqlite")).unwrap();
    s.enqueue("Build inventory").unwrap();
    let claim = s.claim_next("worker").unwrap().unwrap();
    let cancel = CancellationToken::new();
    let run = || ExecutionRun {
        agent: "builder",
        store: &s,
        claim: &claim,
        cancel: &cancel,
    };
    let mut app = draft();
    p.write_app(run(), &app).await.unwrap();
    let preview = p.app_session(&app.id, 1, true).await.unwrap();
    p.activate_app(&app.id, 1, preview["session"].as_str().unwrap(), None)
        .await
        .unwrap();
    let live = p.app_session(&app.id, 1, false).await.unwrap();
    let old = live["session"].as_str().unwrap();
    let record = p
        .session_records(&app.id, old, &save("Retained item"))
        .await
        .unwrap();
    app.expected_revision = 1;
    app.description = "Updated inventory".into();
    app.collections
        .get_mut("items")
        .unwrap()
        .insert("note".into(), ayati::platform::apps::FieldType::String);
    p.write_app(run(), &app).await.unwrap();
    assert_eq!(
        p.session_records(&app.id, old, &list()).await.unwrap()["records"][0]["values"]["name"],
        "Retained item"
    );
    let next = p.app_session(&app.id, 2, true).await.unwrap();
    assert!(
        p.activate_app(&app.id, 2, next["session"].as_str().unwrap(), None)
            .await
            .is_err()
    );
    p.activate_app(&app.id, 2, next["session"].as_str().unwrap(), Some(1))
        .await
        .unwrap();
    assert!(p.session_records(&app.id, old, &list()).await.is_err());
    let live = p.app_session(&app.id, 2, false).await.unwrap();
    p.session_records(&app.id,live["session"].as_str().unwrap(),&command(json!({"action":"save","collection":"items","id":record["id"],"expected_revision":1,"values":{"note":"Keep through rollback"},"request_id":uuid::Uuid::new_v4().to_string()}))).await.unwrap();
    p.activate_app(&app.id, 1, preview["session"].as_str().unwrap(), Some(2))
        .await
        .unwrap();
    let reverted = p.app_session(&app.id, 1, false).await.unwrap();
    assert_eq!(
        p.session_records(&app.id, reverted["session"].as_str().unwrap(), &list())
            .await
            .unwrap()["records"][0]["values"]["note"],
        "Keep through rollback"
    );
}

#[tokio::test]
async fn app_record_access_is_explicit_and_revocation_invalidates_source_authority() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("business");
    let p = Platform::open(&path, None).unwrap();
    let s = Store::open(&dir.path().join("builder.sqlite")).unwrap();
    s.enqueue("Build inventory").unwrap();
    let claim = s.claim_next("worker").unwrap().unwrap();
    let cancel = CancellationToken::new();
    let run = || ExecutionRun {
        agent: "builder",
        store: &s,
        claim: &claim,
        cancel: &cancel,
    };
    let app = draft();
    p.write_app(run(), &app).await.unwrap();
    let preview = p.app_session(&app.id, 1, true).await.unwrap();
    p.activate_app(&app.id, 1, preview["session"].as_str().unwrap(), None)
        .await
        .unwrap();
    assert!(p.agent_app_records(run(), &app.id, &list()).await.is_err());
    p.set_app_grant(&app.id, "builder", "items", true, false)
        .await
        .unwrap();
    assert!(p.agent_app_records(run(), &app.id, &list()).await.is_ok());
    assert!(
        p.agent_app_records(run(), &app.id, &save("Denied"))
            .await
            .is_err()
    );
    p.set_app_grant(&app.id, "builder", "items", false, false)
        .await
        .unwrap();
    assert_eq!(p.authority_revision("builder").unwrap(), 1);
    assert!(p.agent_app_records(run(), &app.id, &list()).await.is_err());
    let db = rusqlite::Connection::open(path.join("platform.sqlite")).unwrap();
    db.execute("UPDATE app_sessions SET expires_ms=1", [])
        .unwrap();
    assert!(
        p.app_document(&app.id, preview["document_id"].as_str().unwrap())
            .is_err()
    );
}
