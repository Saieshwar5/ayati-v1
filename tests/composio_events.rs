use ayati::{
    platform::{Platform, webhook::verify},
    storage::Store,
};
use rusqlite::{Connection, params};
use serde_json::json;

fn seed(platform: &Platform, path: &std::path::Path) {
    let db = Connection::open(path.join("platform.sqlite")).unwrap();
    db.execute("INSERT INTO connections(id,toolkit,label,account_id,auth_config_id,status) VALUES('work','gmail','Work','ca_work','ac_work','ACTIVE')",[]).unwrap();
    for agent in ["sales", "support"] {
        db.execute("INSERT INTO triggers(id,agent_id,connection_id,name,slug,provider_id,instructions,filter,state) VALUES(?1,?2,'work',?2,'GMAIL_NEW_GMAIL_MESSAGE','ti_work','Prepare an owner report','{\"kind\":\"stock\"}','active')",params![format!("trigger-{agent}"),agent]).unwrap();
    }
    assert!(
        platform
            .triggers("foreign")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
}
fn body(p: &Platform, id: &str, kind: &str) -> Vec<u8> {
    json!({"id":id,"type":"composio.trigger.message","metadata":{"user_id":p.user_id,"connected_account_id":"ca_work","trigger_id":"ti_work","trigger_slug":"GMAIL_NEW_GMAIL_MESSAGE"},"data":{"kind":kind,"text":"Untrusted email content"}}).to_string().into_bytes()
}

#[test]
fn matching_delivery_is_scoped_deduplicated_and_recovers_after_queue_pressure() {
    let dir = tempfile::tempdir().unwrap();
    let platform = Platform::open(&dir.path().join("business"), None).unwrap();
    seed(&platform, &dir.path().join("business"));
    assert_eq!(
        platform
            .accept_webhook("msg_wrong", &body(&platform, "msg_wrong", "other"))
            .unwrap(),
        0
    );
    assert_eq!(
        platform
            .accept_webhook("msg_stock", &body(&platform, "msg_stock", "stock"))
            .unwrap(),
        2
    );
    assert_eq!(
        platform
            .accept_webhook("msg_stock", &body(&platform, "msg_stock", "stock"))
            .unwrap(),
        0
    );
    assert!(
        platform
            .accept_webhook("msg_stock", &body(&platform, "msg_stock", "different"))
            .is_err()
    );
    let sales = Store::open(&dir.path().join("sales.sqlite")).unwrap();
    let support = Store::open(&dir.path().join("support.sqlite")).unwrap();
    for _ in 0..32 {
        sales.enqueue("Existing work").unwrap();
    }
    assert!(
        platform
            .dispatch_triggers("sales", &sales)
            .unwrap()
            .is_empty()
    );
    assert!(
        platform
            .dispatch_triggers("foreign", &support)
            .unwrap()
            .is_empty()
    );
    let supported = platform.dispatch_triggers("support", &support).unwrap();
    assert_eq!(supported.len(), 1);
    assert_eq!(support.pending_count().unwrap(), 1);
    assert!(platform.trigger_runs("sales", "trigger-support").is_err());
    let first = sales.tasks().unwrap().pop().unwrap();
    sales.stop_work(&first.id).unwrap();
    let accepted = platform.dispatch_triggers("sales", &sales).unwrap();
    assert_eq!(accepted.len(), 1);
    assert!(
        sales
            .user_text(&accepted[0])
            .unwrap()
            .contains("Untrusted external event data")
    );
    assert!(
        platform
            .dispatch_triggers("sales", &sales)
            .unwrap()
            .is_empty()
    );
    drop(platform);
    let platform = Platform::open(&dir.path().join("business"), None).unwrap();
    assert!(
        platform
            .dispatch_triggers("sales", &sales)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        platform
            .accept_webhook("msg_stock", &body(&platform, "msg_stock", "stock"))
            .unwrap(),
        0
    );
    assert_eq!(
        platform
            .triggers("support")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn pausing_and_revocation_fence_only_the_owning_agents_waiting_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("business");
    let platform = Platform::open(&path, None).unwrap();
    seed(&platform, &path);
    platform
        .accept_webhook("msg_stock", &body(&platform, "msg_stock", "stock"))
        .unwrap();
    let sales = Store::open(&dir.path().join("sales.sqlite")).unwrap();
    let support = Store::open(&dir.path().join("support.sqlite")).unwrap();
    let task = platform
        .dispatch_triggers("sales", &sales)
        .unwrap()
        .remove(0);
    assert!(
        platform
            .set_trigger_state("support", "trigger-sales", "paused", 1)
            .await
            .is_err()
    );
    platform
        .set_trigger_state("sales", "trigger-sales", "paused", 1)
        .await
        .unwrap();
    platform.fence_trigger_jobs("sales", &sales).unwrap();
    assert_eq!(sales.task(&task).unwrap().unwrap().status, "stopped");
    assert!(
        platform
            .set_trigger_state("sales", "trigger-sales", "active", 1)
            .await
            .is_err()
    );
    assert_eq!(
        platform
            .dispatch_triggers("support", &support)
            .unwrap()
            .len(),
        1
    );
    platform.revoke_connection("work").await.unwrap();
    platform.fence_trigger_jobs("support", &support).unwrap();
    assert!(support.claim_next("worker").unwrap().is_none());
    assert!(
        platform
            .dispatch_triggers("sales", &sales)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unsigned_stale_future_and_tampered_webhooks_are_rejected() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ring::hmac;
    let secret = "fixture-webhook-secret";
    let body = br#"{"id":"msg_test"}"#;
    let mut input = b"msg_test.1700000000.".to_vec();
    input.extend_from_slice(body);
    let signature = STANDARD.encode(hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes()),
        &input,
    ));
    let signature = format!("v1,{signature}");
    assert!(
        verify(
            secret,
            "msg_test",
            "1700000000",
            &signature,
            body,
            1700000000
        )
        .is_ok()
    );
    assert!(
        verify(
            secret,
            "msg_test",
            "1700000000",
            &format!("v1,bad {signature}"),
            body,
            1700000000
        )
        .is_ok()
    );
    assert!(
        verify(
            secret,
            "msg_test",
            "1700000000",
            &signature,
            body,
            1700000301
        )
        .is_err()
    );
    assert!(
        verify(
            secret,
            "msg_test",
            "1700000000",
            &signature,
            body,
            1699999699
        )
        .is_err()
    );
    assert!(
        verify(
            secret,
            "msg_test",
            "1700000000",
            &signature,
            b"modified",
            1700000000
        )
        .is_err()
    );
    assert!(verify(secret, "msg_test", "1700000000", "", body, 1700000000).is_err());
    assert!(
        verify(
            "short",
            "msg_test",
            "1700000000",
            &signature,
            body,
            1700000000
        )
        .is_err()
    );
}

#[tokio::test]
async fn editing_a_rule_fences_old_queued_work_and_event_text_is_not_owner_memory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("business");
    let platform = Platform::open(&path, None).unwrap();
    seed(&platform, &path);
    let sales = Store::open(&dir.path().join("sales.sqlite")).unwrap();
    platform
        .accept_webhook("msg_old_rule", &body(&platform, "msg_old_rule", "stock"))
        .unwrap();
    let old = platform
        .dispatch_triggers("sales", &sales)
        .unwrap()
        .remove(0);
    platform
        .edit_trigger(
            "sales",
            "trigger-sales",
            "New report",
            "Use the revised owner task",
            1,
        )
        .await
        .unwrap();
    platform.fence_trigger_jobs("sales", &sales).unwrap();
    assert_eq!(sales.task(&old).unwrap().unwrap().status, "stopped");
    assert!(
        platform
            .edit_trigger("sales", "trigger-sales", "Stale", "Stale", 1)
            .await
            .is_err()
    );
    platform
        .accept_webhook("msg_new_rule", &body(&platform, "msg_new_rule", "stock"))
        .unwrap();
    let new = platform
        .dispatch_triggers("sales", &sales)
        .unwrap()
        .remove(0);
    assert!(
        sales
            .user_text(&new)
            .unwrap()
            .contains("Use the revised owner task")
    );
    let claim = sales.claim_next("worker").unwrap().unwrap();
    assert_eq!(claim.task_id, new);
    assert!(sales.memory_source(&claim).is_err());
}
