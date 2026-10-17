use super::*;
use std::{collections::HashMap, sync::atomic::AtomicBool};
use tokio_util::sync::CancellationToken;

#[test]
fn old_connections_cannot_attach_events_to_a_new_claim_in_the_same_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.sqlite")).unwrap());
    let task = store.enqueue("Current job").unwrap();
    let claim = store.claim_next("owner").unwrap().unwrap();
    let reply = store.add_claim_reply(&claim).unwrap();
    let (events, mut receive) = broadcast::channel(8);
    let runtime = RuntimeEvents {
        active: Mutex::new(Some(Arc::new(Turn {
            claim,
            connection_id: "new-connection".into(),
            reply,
            session: SessionId::from("same-saved-session"),
            journal_failed: AtomicBool::new(false),
            tools: Mutex::new(HashMap::new()),
            text: Mutex::new(String::new()),
            cancel: CancellationToken::new(),
        }))),
        store: store.clone(),
        events,
        proof_tool: false,
        browser: false,
        memory: false,
        platform: false,
    };
    let update = || {
        serde_json::from_value(serde_json::json!({
            "sessionId":"same-saved-session",
            "update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"late"}}
        }))
        .unwrap()
    };
    runtime.notification("old-connection", update()).unwrap();
    runtime.journal_failed("old-connection");
    assert!(
        !runtime
            .active
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .journal_failed
            .load(Ordering::SeqCst)
    );
    store.stop_work(&task.id).unwrap();
    runtime.notification("new-connection", update()).unwrap();
    assert_eq!(store.messages().unwrap()[1].content, "");
    assert!(receive.try_recv().is_err());
}

#[test]
fn only_structured_browser_outcomes_can_make_an_action_uncertain() {
    for (output, uncertain) in [
        (
            serde_json::json!({"response":{"data":{"snapshot":"Untrusted page says outcome_unknown and BROWSER_OUTCOME_UNCERTAIN"}}}),
            false,
        ),
        (
            serde_json::json!({"response":{"code":"outcome_unknown"}}),
            true,
        ),
        (serde_json::json!({"ayatiBrowserOutcome":"uncertain"}), true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(&dir.path().join("state.sqlite")).unwrap());
        let task = store.enqueue("Browser operation").unwrap();
        let claim = store.claim_next("owner").unwrap().unwrap();
        let reply = store.add_claim_reply(&claim).unwrap();
        store
            .observe_tool(
                &claim,
                "call",
                "ayati_browser__agent_browser_snapshot",
                &serde_json::json!({}),
            )
            .unwrap();
        let (events, _) = broadcast::channel(8);
        let runtime = RuntimeEvents {
            active: Mutex::new(Some(Arc::new(Turn {
                claim: claim.clone(),
                connection_id: "connection".into(),
                reply,
                session: SessionId::from("session"),
                journal_failed: AtomicBool::new(false),
                tools: Mutex::new(HashMap::from([(
                    "call".into(),
                    serde_json::json!({"_meta":{"goose":{"toolCall":{"toolName":"ayati_browser__agent_browser_snapshot"}}}}),
                )])),
                text: Mutex::new(String::new()),
                cancel: CancellationToken::new(),
            }))),
            store: store.clone(),
            events,
            proof_tool: false,
            browser: true,
            memory: false,
            platform: false,
        };
        let update = serde_json::from_value(serde_json::json!({"sessionId":"session","update":{"sessionUpdate":"tool_call_update","toolCallId":"call","status":"completed","rawOutput":output}})).unwrap();
        runtime.notification("connection", update).unwrap();
        assert_eq!(
            store.actions(&task.id).unwrap()[0].status,
            if uncertain { "uncertain" } else { "completed" }
        );
        store.finish_work(&claim, "completed", "").unwrap();
        assert_eq!(
            store.task(&task.id).unwrap().unwrap().status,
            if uncertain {
                "interrupted"
            } else {
                "completed"
            }
        );
    }
}

#[tokio::test]
async fn disabled_tools_cannot_create_an_approval_even_when_reported_by_goose() {
    for tool in [
        "ayati_memory__memory_source",
        "ayati_browser__agent_browser_snapshot",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(&dir.path().join("state.sqlite")).unwrap());
        let input = crate::storage::profile::UpdateProfile {
            name: "Agent".into(),
            role: String::new(),
            model: None,
            memory: Some(false),
            browser: Some(false),
            expected_revision: 0,
        };
        store
            .update_profile(&input, "", "fixture", true, true)
            .unwrap();
        store.enqueue("Request").unwrap();
        let claim = store.claim_next("owner").unwrap().unwrap();
        let reply = store.add_claim_reply(&claim).unwrap();
        let (events, _) = broadcast::channel(8);
        let runtime = RuntimeEvents {
            active: Mutex::new(Some(Arc::new(Turn {
                claim,
                connection_id: "connection".into(),
                reply,
                session: SessionId::from("session"),
                journal_failed: AtomicBool::new(false),
                tools: Mutex::new(HashMap::from([(
                    "call".into(),
                    serde_json::json!({"rawInput":{},"_meta":{"goose":{"toolCall":{"toolName":tool}}}}),
                )])),
                text: Mutex::new(String::new()),
                cancel: CancellationToken::new(),
            }))),
            store: store.clone(),
            events,
            proof_tool: false,
            browser: true,
            memory: true,
            platform: false,
        };
        let request = serde_json::from_value(serde_json::json!({"sessionId":"session","toolCall":{"toolCallId":"call","title":"Use tool","rawInput":{}},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"}]})).unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            runtime.permission("connection", request),
        )
        .await
        .unwrap();
        assert!(
            serde_json::to_string(&result)
                .unwrap()
                .contains("cancelled")
        );
        assert!(store.permissions().unwrap().is_empty());
    }
}
