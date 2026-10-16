use super::*;
fn broker() -> Broker {
    Broker {
        upstream: None,
        owner: None,
        enabled: false,
        run: None,
        fresh: false,
        poisoned: false,
        parked: false,
        interaction_pending: false,
        closed: false,
    }
}

#[tokio::test]
async fn late_view_requests_cannot_relaunch_a_browser_after_it_was_parked() {
    let mut broker = broker();
    broker.handle(request(json!({"op":"park"}))).await.unwrap();
    assert!(broker.handle(request(json!({"op":"frame"}))).await.is_err());
    assert!(
        broker
            .handle(request(json!({"op":"arm","token":"run"})))
            .await
            .is_err()
    );
    assert!(broker.upstream.is_none());
}
fn request(value: Value) -> Request {
    serde_json::from_value(value).unwrap()
}

#[tokio::test]
async fn stopped_and_previous_run_commands_cannot_execute_after_rearm() {
    let mut broker = broker();
    let action = || {
        request(
            json!({"op":"agent","token":"old-run","tool":"agent_browser_open","arguments":{"url":"https://example.com"}}),
        )
    };
    assert!(broker.handle(action()).await.is_err());
    broker
        .handle(request(json!({"op":"arm","token":"old-run"})))
        .await
        .unwrap();
    broker
        .handle(request(json!({"op":"disarm"})))
        .await
        .unwrap();
    assert!(broker.handle(action()).await.is_err());
    broker
        .handle(request(json!({"op":"arm","token":"new-run"})))
        .await
        .unwrap();
    assert!(broker.handle(action()).await.is_err());
    assert!(broker.upstream.is_none());
}
#[tokio::test]
async fn owner_mode_blocks_agent_captures_and_foreign_controller_input() {
    let mut broker = broker();
    broker.owner = Some("owner".into());
    broker.enabled = true;
    broker.run = Some("old-run".into());
    assert!(broker.handle(request(json!({"op":"agent","token":"old-run","tool":"agent_browser_snapshot","arguments":{}}))).await.is_err());
    assert!(
        broker
            .handle(request(json!({"op":"arm","token":"new-run"})))
            .await
            .is_err()
    );
    assert!(
        broker
            .handle(request(json!({"op":"frame","token":"foreign"})))
            .await
            .is_err()
    );
    assert!(broker.handle(request(json!({"op":"input","token":"foreign","tool":"agent_browser_press","arguments":{"key":"Enter"}}))).await.is_err());
    assert!(broker.upstream.is_none());
}
#[tokio::test]
async fn malformed_arm_and_stale_references_fail_before_browser_execution() {
    let mut broker = broker();
    assert!(broker.handle(request(json!({"op":"arm"}))).await.is_err());
    assert!(!broker.enabled);
    broker
        .handle(request(json!({"op":"arm","token":"run"})))
        .await
        .unwrap();
    assert!(broker.handle(request(json!({"op":"agent","token":"run","tool":"agent_browser_click","arguments":{"selector":"@e1"}}))).await.is_err());
    assert!(broker.upstream.is_none());
    assert!(
        serde_json::from_value::<Request>(json!({"op":"input","executable":"/bin/sh"})).is_err()
    );
    assert!(validate_input("agent_browser_eval", &json!({"script":"1"})).is_err());
}
