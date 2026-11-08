//! D246: later requests remember an agent handoff whole, or leave it out whole
//! when any part of it is source-bearing (`source_exclusions`).
use super::super::{
    handoff_admission::visibility_tests::{asking, helper},
    handoff_authority_tests::{account, rule},
    handoff_tests::*,
    tests::*,
};
use super::*;
use crate::identity::{
    chat::model::Job,
    control::{broker::invoke, model::ActionRequest},
};

/// Lets both agents read an order; set before a handoff snapshots the sender's rules.
pub(in crate::identity::postgres) async fn order_reads(s: &Setup) -> String {
    let conn = account(s).await;
    rule(s, &s.a, &conn, "order.read", "allow", 0).await;
    rule(s, &s.b, &conn, "order.read", "allow", 0).await;
    conn
}
/// `job` answers from a business operation: one completed order read.
pub(in crate::identity::postgres) async fn read_an_order(s: &Setup, job: &Job, conn: &str) {
    let read = ActionRequest {
        connection: conn.into(),
        operation: "order.read".into(),
        arguments: json!({"order":"one"}),
    };
    let proposed = s.db.control_propose(job, &read).await.unwrap();
    s.db.control_finish(
        &s.w,
        &proposed.dispatch.unwrap(),
        json!({"order":"one","status":"delayed"}),
        true,
    )
    .await
    .unwrap();
}
/// The sender asks, waits, the helper answers (from an order read when `tool`
/// names a connection) and the sender gives its result. Text: [task, answer, result].
async fn exchange(s: &Setup, sender: &Job, key: &str, text: [&str; 3], tool: Option<&str>) {
    let [task, answer, result] = text;
    let receipt =
        s.db.request_handoff(sender, &asking(&s.b, key, task))
            .await
            .unwrap();
    s.db.chat_finish(sender, "Asked the other agent; waiting.", "complete")
        .await
        .unwrap();
    let child = helper(&s.db, &s.w, &receipt).await;
    if let Some(conn) = tool {
        read_an_order(s, &child, conn).await;
    }
    s.db.chat_finish(&child, answer, "complete").await.unwrap();
    let resumed = claim(&s.db, sender).await;
    s.db.chat_finish(&resumed, result, "complete")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn later_requests_remember_a_plain_exchange_whole_and_leave_out_a_tool_backed_one() {
    let s = setup().await;
    let conn = order_reads(&s).await;
    let plain = [
        "PLAIN_ASK how many walnut boards?",
        "PLAIN_ANSWER 42 boards",
        "PLAIN_RESULT reserved 42",
    ];
    exchange(&s, &s.parent, "plain", plain, None).await;
    let second =
        s.db.chat_send(
            &s.owner,
            &s.w,
            &s.c,
            &message("Check order one", Some(&s.a)),
        )
        .await
        .unwrap()
        .unwrap();
    let second = claim(&s.db, &second).await;
    let tool = [
        "TOOL_ASK read order one",
        "TOOL_ANSWER order one is delayed",
        "TOOL_RESULT it is delayed",
    ];
    exchange(&s, &second, "tool", tool, Some(&conn)).await;

    let next =
        s.db.chat_send(
            &s.owner,
            &s.w,
            &s.c,
            &message("What did we agree?", Some(&s.a)),
        )
        .await
        .unwrap()
        .unwrap();
    for remembered in [
        "PLAIN_ASK",
        "PLAIN_ANSWER",
        "PLAIN_RESULT",
        r#""role":"answer""#,
        r#""role":"request""#,
    ] {
        assert!(next.prompt.contains(remembered), "{remembered}");
    }
    assert!(
        next.prompt
            .contains("A message with handoff is part of an agent handoff")
    );
    for left_out in ["TOOL_ASK", "TOOL_ANSWER", "TOOL_RESULT"] {
        assert!(!next.prompt.contains(left_out), "{left_out}");
    }
    // History tools follow the same rule.
    let next = claim(&s.db, &next).await;
    let found = invoke(
        &s.db,
        &next,
        "search_conversation_history",
        json!({"query":""}),
    )
    .await
    .unwrap()
    .to_string();
    assert!(found.contains("PLAIN_ANSWER") && !found.contains("TOOL_ANSWER"));
}
