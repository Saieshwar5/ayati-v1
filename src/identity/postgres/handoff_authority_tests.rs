use super::{handoff_tests::*, tests::*, *};
use crate::identity::{
    chat::model::Job,
    control::{connections_model::SystemApproval, model::ActionRequest},
};
use serde_json::json;
async fn pause(s: &Setup, agent: &str, paused: bool) {
    let current =
        s.db.managed_agent(&s.owner, &s.w, agent, true)
            .await
            .unwrap();
    let mut profile: crate::identity::agents::config::SaveAgent = serde_json::from_value(json!({
        "id":agent,"name":current["name"],"role":current["role"],"model":current["model"],
        "audience":current["audience"],"revision":current["revision"],"settings":current["settings"]}))
    .unwrap();
    profile.settings.paused = paused;
    s.db.save_agent_profile(&s.owner, &s.w, &profile)
        .await
        .unwrap();
}
pub(super) async fn account(s: &Setup) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    s.db.control_system(
        &s.owner,
        &s.w,
        &SystemApproval {
            server: "handoff-orders".into(),
            approved: true,
            revision: 0,
        },
    )
    .await
    .unwrap();
    s.db.control_connect(&s.owner,&s.w,&s.a,&id,json!({"id":"handoff-orders","name":"Orders","operations":[{"id":"order.read","title":"Read order","tool":"read_order","effect":"read","schema":{"type":"object","properties":{"order":{"type":"string"}},"required":["order"],"additionalProperties":false}},{"id":"order.update","title":"Update order","tool":"update_order","effect":"write","schema":{"type":"object","properties":{"order":{"type":"string"},"address":{"type":"string"}},"required":["order","address"],"additionalProperties":false}}]}),&"a".repeat(43)).await.unwrap();
    id
}
/// This agent's own mode for the action (schema 47: rules belong to agents).
pub(super) async fn rule(s: &Setup, agent: &str, conn: &str, op: &str, mode: &str, _revision: i64) {
    super::tool_test_support::agent_rule(&s.db, &s.owner, &s.w, agent, conn, op, mode).await;
}
pub(super) fn write(conn: &str) -> ActionRequest {
    ActionRequest {
        connection: conn.into(),
        operation: "order.update".into(),
        arguments: json!({"order":"one","address":"EXACT_HANDOFF_WRITE"}),
    }
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_receiver_cannot_gain_a_missing_blocked_or_more_permissive_sender_operation() {
    let s = setup().await;
    let conn = account(&s).await;
    rule(&s, &s.b, &conn, "order.update", "allow", 0).await;
    let (_, child) = s.handoff().await;
    let child = claim(&s.db, &child).await;
    assert!(s.db.control_propose(&child, &write(&conn)).await.is_err());
    assert_eq!(
        s.db.control_operations(&child).await.unwrap()["operations"],
        json!([])
    );
    let s = setup().await;
    let conn = account(&s).await;
    rule(&s, &s.a, &conn, "order.update", "ask", 0).await;
    rule(&s, &s.b, &conn, "order.update", "allow", 0).await;
    let second =
        s.db.request_handoff(
            &s.parent,
            &crate::identity::handoffs::model::Request {
                task: "A distinct task under the newly reviewed sender ceiling.".into(),
                ..request(&s.b, "second")
            },
        )
        .await
        .unwrap();
    let job = super::handoff_access::job(&s.db.db, &s.w, second["reply"].as_str().unwrap())
        .await
        .unwrap();
    let child = claim(&s.db, &job).await;
    assert!(s.db.control_propose(&child, &write(&conn)).await.is_err());
    rule(&s, &s.b, &conn, "order.update", "ask", 1).await;
    let proposed = s.db.control_propose(&child, &write(&conn)).await.unwrap();
    assert_eq!(proposed.action["state"], "pending");
    assert!(proposed.dispatch.is_none());
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_completed_actions_are_deduplicated_across_the_entire_logical_task_tree() {
    let s = setup().await;
    let conn = account(&s).await;
    rule(&s, &s.a, &conn, "order.update", "allow", 0).await;
    rule(&s, &s.b, &conn, "order.update", "allow", 0).await;
    let first =
        s.db.control_propose(&s.parent, &write(&conn))
            .await
            .unwrap();
    let dispatch = first.dispatch.unwrap();
    s.db.control_finish(&s.w, &dispatch, json!({"confirmed":true}), true)
        .await
        .unwrap();
    let (_, child) = s.handoff().await;
    let child = claim(&s.db, &child).await;
    let reused = s.db.control_propose(&child, &write(&conn)).await.unwrap();
    assert_eq!(reused.action["id"], first.action["id"]);
    assert_eq!(reused.action["state"], "completed");
    assert!(reused.dispatch.is_none());
    assert_eq!(
        query(
            &s.db.db,
            "SELECT count(*) AS n FROM relay_control_actions",
            vec![]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap(),
        1
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_unresolved_questions_approvals_and_uncertain_writes_cannot_be_evaded_by_delegation()
 {
    let s = setup().await;
    let conn = account(&s).await;
    rule(&s, &s.a, &conn, "order.update", "ask", 0).await;
    let pending =
        s.db.control_propose(&s.parent, &write(&conn))
            .await
            .unwrap();
    assert_eq!(pending.action["state"], "pending");
    assert!(
        s.db.request_handoff(&s.parent, &request(&s.b, "evade-approval"))
            .await
            .is_err()
    );
    let s = setup().await;
    let conn = account(&s).await;
    rule(&s, &s.a, &conn, "order.update", "allow", 0).await;
    let action =
        s.db.control_propose(&s.parent, &write(&conn))
            .await
            .unwrap();
    s.db.control_finish(
        &s.w,
        &action.dispatch.unwrap(),
        json!({"unknown":true}),
        false,
    )
    .await
    .unwrap();
    assert!(
        s.db.request_handoff(&s.parent, &request(&s.b, "evade-uncertain"))
            .await
            .is_err()
    );
    let s = setup().await;
    s.db.ask_question(&s.parent, &super::attention_tests::ask())
        .await
        .unwrap();
    assert!(
        s.db.request_handoff(&s.parent, &request(&s.b, "evade-question"))
            .await
            .is_err()
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_sender_ceiling_revocation_fences_child_approval_and_regrant_cannot_revive_it() {
    let s = setup().await;
    let conn = account(&s).await;
    for agent in [&s.a, &s.b] {
        rule(&s, agent, &conn, "order.update", "ask", 0).await;
    }
    let (_, child) = s.handoff().await;
    s.db.chat_finish(&s.parent, "Waiting for helper.", "complete")
        .await
        .unwrap();
    let child = claim(&s.db, &child).await;
    let action = s.db.control_propose(&child, &write(&conn)).await.unwrap();
    let id = action.action["id"].as_str().unwrap();
    s.db.chat_finish(&child, "Waiting for exact approval.", "complete")
        .await
        .unwrap();
    rule(&s, &s.a, &conn, "order.update", "block", 1).await;
    assert_eq!(state(&s.db, &child).await, "finished");
    assert_eq!(state(&s.db, &s.parent).await, "queued");
    assert!(
        s.db.control_decide(
            &s.owner,
            &s.w,
            &s.b,
            id,
            &super::control_tests::decision("approve")
        )
        .await
        .is_err()
    );
    rule(&s, &s.a, &conn, "order.update", "ask", 2).await;
    assert_eq!(state(&s.db, &child).await, "finished");
    let parent = claim(&s.db, &s.parent).await;
    let replay =
        s.db.request_handoff(&parent, &request(&s.b, "helper"))
            .await
            .unwrap();
    assert_eq!(replay["state"], "cancelled");
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_queue_pressure_rolls_back_message_and_receipt_then_exact_retry_can_succeed() {
    let s = setup().await;
    let mut first = None;
    for _ in 0..32 {
        let j =
            s.db.chat_background(
                &s.owner,
                &s.w,
                &s.c,
                &message("Queued background work", Some(&s.b)),
            )
            .await
            .unwrap()
            .unwrap();
        if first.is_none() {
            first = Some(j.reply);
        }
    }
    let before = s.db.chat_page(&s.owner, &s.w, &s.c, None).await.unwrap();
    let input = request(&s.b, "pressure");
    assert!(
        s.db.request_handoff(&s.parent, &input)
            .await
            .unwrap_err()
            .is::<crate::identity::chat::model::BackgroundQueueFull>()
    );
    assert_eq!(
        s.db.chat_page(&s.owner, &s.w, &s.c, None).await.unwrap(),
        before
    );
    assert!(
        query(&s.db.db, "SELECT id FROM relay_agent_handoffs", vec![])
            .await
            .unwrap()
            .is_none()
    );
    s.db.cancel_assignment(&s.owner, &s.w, &s.c, &first.unwrap())
        .await
        .unwrap();
    s.db.request_handoff(&s.parent, &input).await.unwrap();
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_archive_blocks_new_delegation_but_preserves_already_accepted_helper_continuation()
{
    let s = setup().await;
    let (_, child) = s.handoff().await;
    s.db.chat_finish(&s.parent, "Waiting for helper.", "complete")
        .await
        .unwrap();
    s.db.channel_lifecycle(
        &s.owner,
        &s.w,
        &s.c,
        Some(&crate::identity::chat::organization_model::Lifecycle {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            archived: true,
        }),
    )
    .await
    .unwrap();
    let child = claim(&s.db, &child).await;
    s.db.chat_finish(&child, "Completed accepted work after archive.", "complete")
        .await
        .unwrap();
    assert_eq!(state(&s.db, &s.parent).await, "queued");
    let parent = claim(&s.db, &s.parent).await;
    assert!(
        s.db.request_handoff(
            &parent,
            &crate::identity::handoffs::model::Request {
                task: "A distinct task after archive.".into(),
                ..request(&s.b, "new-after-archive")
            }
        )
        .await
        .is_err()
    );
    assert!(
        s.db.task_continuation(&parent)
            .await
            .unwrap()
            .unwrap()
            .contains("Completed accepted work after archive")
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_requester_requires_each_agent_and_pause_resume_does_not_restore_the_old_child() {
    let s = setup().await;
    let member = user(&s.db, "handoff-requester").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    s.db.channel_members(
        &s.owner,
        &s.w,
        &s.c,
        &[s.owner.id.clone(), member.id.clone()],
    )
    .await
    .unwrap();
    s.db.chat_grant(&s.owner, &s.w, &member.id, &s.a, true)
        .await
        .unwrap();
    let job =
        s.db.chat_send(
            &member,
            &s.w,
            &s.c,
            &message("Member coordination task", Some(&s.a)),
        )
        .await
        .unwrap()
        .unwrap();
    let parent = claim(&s.db, &job).await;
    // A channel member may ask the channel's agents (D231); a paused helper may not.
    pause(&s, &s.b, true).await;
    assert!(
        s.db.request_handoff(&parent, &request(&s.b, "member-helper"))
            .await
            .is_err()
    );
    pause(&s, &s.b, false).await;
    let receipt =
        s.db.request_handoff(&parent, &request(&s.b, "member-helper"))
            .await
            .unwrap();
    let child: Job = super::handoff_access::job(&s.db.db, &s.w, receipt["reply"].as_str().unwrap())
        .await
        .unwrap();
    s.db.chat_finish(&parent, "Waiting.", "complete")
        .await
        .unwrap();
    pause(&s, &s.b, true).await;
    assert_eq!(state(&s.db, &child).await, "finished");
    pause(&s, &s.b, false).await;
    assert_eq!(state(&s.db, &child).await, "finished");
    assert_eq!(state(&s.db, &parent).await, "queued");
}
