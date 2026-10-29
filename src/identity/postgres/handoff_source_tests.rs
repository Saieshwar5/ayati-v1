use super::{
    handoff_authority_tests::{account, rule},
    handoff_tests::*,
    *,
};
use crate::identity::{
    chat::model::Job,
    events::{catalog::Source, model::Trigger},
};
use serde_json::json;
async fn event_setup() -> (Setup, String, Source) {
    let mut s = setup().await;
    s.db.cancel_assignment(&s.owner, &s.w, &s.c, &s.parent.reply)
        .await
        .unwrap();
    let conn = account(&s).await;
    rule(&s, &s.a, &conn, "order.read", "allow", 0).await;
    let source = crate::identity::events::test_catalog::register(Source {
        id: uuid::Uuid::new_v4().to_string(),
        workspace: s.w.clone(),
        connection: conn.clone(),
        connection_fingerprint: "a".repeat(43),
        name: "Handoff source".into(),
        provider: "composio_v3".into(),
        user_id: "person-1".into(),
        connected_account_id: "account-1".into(),
        auth_config_id: "auth-1".into(),
        trigger_id: "trigger-1".into(),
        trigger_slug: "ORDER_CHANGED".into(),
        read_operation: "order.read".into(),
        fields: vec!["order_id".into(), "status".into()],
        secret_file: String::new(),
    })
    .unwrap();
    let input = Trigger {
        id: uuid::Uuid::new_v4().to_string(),
        source: source.id.clone(),
        binding: source.fingerprint().unwrap(),
        chat: s.c.clone(),
        name: "Source task".into(),
        instructions: "Review the event and obtain help if needed.".into(),
        connection_revision: 1,
        read_revision: 1,
        delegation_revision: None,
        filter: json!({}),
    };
    s.db.save_event_trigger(&s.owner, &s.w, &s.a, &input, Some(&source))
        .await
        .unwrap();
    s.db.accept_event(
        &source,
        "source-event",
        &super::event_tests::payload("source-event", "delayed"),
        100000,
    )
    .await
    .unwrap();
    s.db.dispatch_events().await.unwrap();
    let row = query(
        &s.db.db,
        "SELECT reply FROM relay_event_deliveries WHERE event_id='source-event'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap();
    let task =
        super::handoff_access::job(&s.db.db, &s.w, &row.try_get::<String>("", "reply").unwrap())
            .await
            .unwrap();
    s.parent = claim(&s.db, &task).await;
    (s, conn, source)
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_event_projection_requires_the_receiver_own_allow_read_and_source_guard_survives_returned_parent_output()
 {
    let (s, conn, source) = event_setup().await;
    assert!(
        s.db.request_handoff(&s.parent, &request(&s.b, "helper"))
            .await
            .is_err()
    );
    rule(&s, &s.b, &conn, "order.read", "ask", 0).await;
    assert!(
        s.db.request_handoff(&s.parent, &request(&s.b, "helper"))
            .await
            .is_err()
    );
    rule(&s, &s.b, &conn, "order.read", "allow", 1).await;
    let (r, child) = s.handoff().await;
    let request_id = query(
        &s.db.db,
        "SELECT request_message FROM relay_agent_handoffs WHERE id=$1",
        vec![r["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<String>("", "request_message")
    .unwrap();
    s.db.chat_finish(&s.parent, "Waiting for help on the event.", "complete")
        .await
        .unwrap();
    let child = claim(&s.db, &child).await;
    assert!(
        json!(s.db.event_job_context(&child).await.unwrap())
            .to_string()
            .contains("PRIVATE-ORDER-42")
    );
    s.db.chat_finish(&child, "PRIVATE_HELPER_SOURCE_RESULT", "complete")
        .await
        .unwrap();
    let parent = claim(&s.db, &s.parent).await;
    assert!(
        s.db.task_continuation(&parent)
            .await
            .unwrap()
            .unwrap()
            .contains("PRIVATE_HELPER_SOURCE_RESULT")
    );
    s.db.chat_finish(&parent, "PRIVATE_PARENT_DERIVED_RESULT", "complete")
        .await
        .unwrap();
    assert_eq!(
        s.db.response_body(&s.owner, &s.w, &s.c, &parent.reply, false)
            .await
            .unwrap()["source_scope"],
        source.id
    );
    assert_eq!(
        s.db.response_body(&s.owner, &s.w, &s.c, &request_id, false)
            .await
            .unwrap()["source_scope"],
        source.id
    );
    rule(&s, &s.b, &conn, "order.read", "block", 2).await;
    for id in [&parent.reply, &child.reply, &request_id] {
        assert!(
            s.db.response_body(&s.owner, &s.w, &s.c, id, false)
                .await
                .is_err()
        );
    }
    let history = s.db.chat_page(&s.owner, &s.w, &s.c, None).await.unwrap();
    assert!(!history.to_string().contains("PRIVATE_HELPER_SOURCE_RESULT"));
    assert!(
        !history
            .to_string()
            .contains("PRIVATE_PARENT_DERIVED_RESULT")
    );
    rule(&s, &s.b, &conn, "order.read", "allow", 3).await;
    assert!(
        s.db.response_body(&s.owner, &s.w, &s.c, &parent.reply, false)
            .await
            .is_err()
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_helper_read_revocation_stops_parent_and_child_question_and_keeps_old_tree_stopped_after_regrant()
 {
    let (s, conn, source) = event_setup().await;
    rule(&s, &s.b, &conn, "order.read", "allow", 0).await;
    let (_, child) = s.handoff().await;
    s.db.chat_finish(&s.parent, "Waiting for helper.", "complete")
        .await
        .unwrap();
    let child = claim(&s.db, &child).await;
    let mut ask = super::attention_tests::ask();
    ask.specification.title = "PRIVATE_HELPER_QUESTION".into();
    let q = s.db.ask_question(&child, &ask).await.unwrap();
    let id = q["id"].as_str().unwrap();
    s.db.chat_finish(&child, "Waiting for a source-specific answer.", "complete")
        .await
        .unwrap();
    assert_eq!(
        s.db.question(&s.owner, &s.w, id).await.unwrap()["source_scope"],
        source.id
    );
    rule(&s, &s.b, &conn, "order.read", "block", 1).await;
    assert_eq!(state(&s.db, &s.parent).await, "finished");
    assert_eq!(state(&s.db, &child).await, "finished");
    assert!(s.db.question(&s.owner, &s.w, id).await.is_err());
    assert!(
        s.db.attention(&s.owner, &s.w, &super::attention_tests::page("needs_you"))
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    rule(&s, &s.b, &conn, "order.read", "allow", 2).await;
    assert!(s.db.event_job_context(&child).await.is_err());
    assert_eq!(s.db.reconcile_handoffs().await.unwrap(), 0);
    let viewer = Job {
        claim: None,
        ..s.parent.clone()
    };
    let tx = s.db.read_tx(&s.w).await.unwrap();
    assert!(!super::source_access::allowed(&tx, &viewer).await.unwrap());
    tx.commit().await.unwrap();
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_source_messages_and_returned_results_never_enter_an_independent_later_prompt() {
    let (s, conn, _) = event_setup().await;
    rule(&s, &s.b, &conn, "order.read", "allow", 0).await;
    let mut input = request(&s.b, "private-instruction");
    input.task = "PRIVATE_HANDOFF_INSTRUCTION with copied authorized event data".into();
    let receipt = s.db.request_handoff(&s.parent, &input).await.unwrap();
    let child = super::handoff_access::job(&s.db.db, &s.w, receipt["reply"].as_str().unwrap())
        .await
        .unwrap();
    s.db.chat_finish(&s.parent, "Waiting.", "complete")
        .await
        .unwrap();
    let child = claim(&s.db, &child).await;
    s.db.chat_finish(&child, "PRIVATE_HELPER_OUTPUT", "complete")
        .await
        .unwrap();
    let parent = claim(&s.db, &s.parent).await;
    s.db.chat_finish(&parent, "PRIVATE_PARENT_OUTPUT", "complete")
        .await
        .unwrap();
    let next =
        s.db.chat_send(
            &s.owner,
            &s.w,
            &s.c,
            &super::tests::message("An independent request", Some(&s.a)),
        )
        .await
        .unwrap()
        .unwrap();
    for marker in [
        "PRIVATE_HANDOFF_INSTRUCTION",
        "PRIVATE_HELPER_OUTPUT",
        "PRIVATE_PARENT_OUTPUT",
        "PRIVATE-ORDER-42",
    ] {
        assert!(!next.prompt.contains(marker));
    }
}
