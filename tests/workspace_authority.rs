use ayati::storage::Store;
use serde_json::json;

#[test]
fn business_calls_require_an_exact_unused_active_approval() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.sqlite")).unwrap();
    store.enqueue("Read my inbox").unwrap();
    let claim = store.claim_next("owner").unwrap().unwrap();
    let tool = "ayati_workspace__connector_execute";
    let args = json!({"connection_id":"work","tool":"GMAIL_FETCH_EMAILS","arguments":{}});
    store.observe_tool(&claim, "call", tool, &args).unwrap();
    let permission = store
        .add_claim_permission(
            &claim,
            "call",
            "Read inbox",
            &json!({"tool":tool,"request":{"rawInput":args}}),
        )
        .unwrap();
    assert!(
        store
            .consume_workspace_permission(&claim, tool, &args)
            .is_err()
    );
    store.decide_permission(&permission, true).unwrap();
    let changed = json!({"connection_id":"foreign","tool":"GMAIL_FETCH_EMAILS","arguments":{}});
    assert!(
        store
            .consume_workspace_permission(&claim, tool, &changed)
            .is_err()
    );
    assert!(
        store
            .consume_workspace_permission(&claim, "ayati_workspace__connector_tools", &json!({}))
            .is_err()
    );
    assert_eq!(
        store
            .consume_workspace_permission(&claim, tool, &args)
            .unwrap(),
        "call"
    );
    assert!(
        store
            .consume_workspace_permission(&claim, tool, &args)
            .is_err()
    );
    store.observe_tool(&claim, "next", tool, &args).unwrap();
    let permission = store
        .add_claim_permission(
            &claim,
            "next",
            "Read inbox",
            &json!({"tool":tool,"request":{"rawInput":args}}),
        )
        .unwrap();
    store.decide_permission(&permission, true).unwrap();
    store.stop_work(&claim.task_id).unwrap();
    assert!(
        store
            .consume_workspace_permission(&claim, tool, &args)
            .is_err()
    );
}

#[test]
fn adding_workspace_tools_resets_only_an_otherwise_compatible_context() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.sqlite")).unwrap();
    let before = json!({"provider":"openai","model":"original","adapter":1}).to_string();
    let after = json!({"provider":"openai","model":"original","adapter":1,"workspace_adapter":1})
        .to_string();
    store.save_runtime_session("old", &before).unwrap();
    let wrong = json!({"provider":"openai","model":"different","adapter":1,"workspace_adapter":1})
        .to_string();
    assert!(store.workspace_runtime_session(&wrong).is_err());
    assert_eq!(
        store.runtime_session(&before).unwrap().as_deref(),
        Some("old")
    );
    assert!(store.workspace_runtime_session(&after).unwrap().is_none());
    store.save_runtime_session("new", &after).unwrap();
    assert_eq!(
        store.workspace_runtime_session(&after).unwrap().as_deref(),
        Some("new")
    );
    let revoked=json!({"provider":"openai","model":"original","adapter":1,"workspace_adapter":1,"authority_revision":1}).to_string();
    assert!(store.workspace_runtime_session(&revoked).unwrap().is_none());
    store
        .save_runtime_session("after-revocation", &revoked)
        .unwrap();
    assert!(
        store.workspace_runtime_session(&after).is_err(),
        "Authority revisions must never roll back silently"
    );
}

#[test]
fn composio_does_not_allow_keys_to_be_redirected_to_arbitrary_endpoints() {
    use ayati::platform::composio::Composio;
    for endpoint in [
        "https://attacker.example/api/",
        "http://backend.composio.dev/api/",
        "https://backend.composio.dev@attacker.example/",
        "https://backend.composio.dev/api/?key=secret",
    ] {
        assert!(Composio::new(endpoint, "fixture".into()).is_err());
    }
    let consumer =
        Composio::new("http://127.0.0.1:1/", "ck_synthetic_not_a_real_key".into()).unwrap();
    assert!(consumer.configuration_error().is_none());
    assert!(consumer.is_consumer());
}
