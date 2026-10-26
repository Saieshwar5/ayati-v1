use super::{event_tests::*, tests::*, *};
use serde_json::json;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_read_revocation_hides_completed_history_reader_work_attention_and_never_regrants_old_data()
 {
    let s = setup().await;
    s.save().await;
    s.accept("complete", "delayed").await;
    s.db.dispatch_events().await.unwrap();
    let job = s.claim().await;
    s.db.chat_finish(&job, "PRIVATE-EVENT-RESULT", "complete")
        .await
        .unwrap();
    assert!(
        s.db.chat_page(&s.owner, &s.w, &s.c, None)
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE-EVENT-RESULT")
    );
    let before = s.db.sync_page(&s.owner, &s.w, None).await.unwrap()["cursor"]
        .as_i64()
        .unwrap();
    s.db.control_rule(
        &s.owner,
        &s.w,
        &s.a,
        &rule(&s.source.connection, &s.c, "block", 1),
    )
    .await
    .unwrap();
    assert!(
        s.db.response_body(&s.owner, &s.w, &s.c, &job.reply, false)
            .await
            .is_err()
    );
    assert!(
        s.db.chat_focus(&s.owner, &s.w, &s.c, &job.reply)
            .await
            .is_err()
    );
    let history = s.db.chat_page(&s.owner, &s.w, &s.c, None).await.unwrap();
    assert!(!history.to_string().contains("PRIVATE-EVENT-RESULT"));
    assert_eq!(
        history["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == job.reply)
            .unwrap()["response_unavailable"],
        true
    );
    assert!(
        s.db.agent_request(&s.owner, &s.w, &s.a, &job.reply)
            .await
            .is_err()
    );
    assert!(
        !s.db
            .attention(&s.owner, &s.w, &super::attention_tests::page("recent"))
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE-EVENT-RESULT")
    );
    let changes = s.db.sync_page(&s.owner, &s.w, Some(before)).await.unwrap();
    assert!(
        changes["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "event_access" && e["source"] == s.source.id)
    );
    assert!(!changes.to_string().contains("PRIVATE-EVENT-RESULT"));
    s.db.control_rule(
        &s.owner,
        &s.w,
        &s.a,
        &rule(&s.source.connection, &s.c, "allow", 2),
    )
    .await
    .unwrap();
    assert!(
        s.db.response_body(&s.owner, &s.w, &s.c, &job.reply, false)
            .await
            .is_err()
    );
    assert_eq!(
        s.db.event_trigger_details(
            &s.owner,
            &s.w,
            &s.a,
            &s.input.id,
            std::slice::from_ref(&s.source)
        )
        .await
        .unwrap()["state"],
        "blocked"
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_pending_question_source_and_answer_are_denied_after_read_revocation_even_after_regrant()
 {
    let s = setup().await;
    s.save().await;
    s.accept("question", "delayed").await;
    s.db.dispatch_events().await.unwrap();
    let job = s.claim().await;
    let mut ask = super::attention_tests::ask();
    ask.specification.title = "PRIVATE-EVENT-QUESTION".into();
    let q = s.db.ask_question(&job, &ask).await.unwrap();
    let id = q["id"].as_str().unwrap();
    s.db.chat_finish(&job, "PRIVATE-EVENT-PROGRESS", "complete")
        .await
        .unwrap();
    assert_eq!(
        s.db.question(&s.owner, &s.w, id).await.unwrap()["source_scope"],
        s.source.id
    );
    assert_eq!(
        s.db.agent_profiles(&s.owner, &s.w).await.unwrap()[0]["pending"],
        1
    );
    s.db.control_rule(
        &s.owner,
        &s.w,
        &s.a,
        &rule(&s.source.connection, &s.c, "block", 1),
    )
    .await
    .unwrap();
    assert!(s.db.question(&s.owner, &s.w, id).await.is_err());
    assert_eq!(
        s.db.agent_profiles(&s.owner, &s.w).await.unwrap()[0]["pending"],
        0
    );
    assert!(
        !s.db
            .chat_page(&s.owner, &s.w, &s.c, None)
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE-EVENT")
    );
    assert!(
        s.db.attention(&s.owner, &s.w, &super::attention_tests::page("needs_you"))
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    s.db.control_rule(
        &s.owner,
        &s.w,
        &s.a,
        &rule(&s.source.connection, &s.c, "allow", 2),
    )
    .await
    .unwrap();
    let answer = crate::identity::interactions::model::Submission {
        id: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        decision: "answer".into(),
        values: json!({"choices":["road"]}),
    };
    assert!(
        s.db.answer_question(&s.owner, &s.w, id, &answer)
            .await
            .is_err()
    );
    assert_eq!(
        query(
            &s.db.db,
            "SELECT state FROM relay_chat_jobs WHERE reply=$1",
            vec![job.reply.into()]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "state")
        .unwrap(),
        "finished"
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_approval_inbox_details_tools_and_decision_require_the_original_source() {
    let s = setup().await;
    let mut write = rule(&s.source.connection, &s.c, "ask", 0);
    write.operation = "order.update".into();
    s.db.control_rule(&s.owner, &s.w, &s.a, &write)
        .await
        .unwrap();
    s.save().await;
    s.accept("approval", "delayed").await;
    s.db.dispatch_events().await.unwrap();
    let job = s.claim().await;
    let proposed=s.db.control_propose(&job,&crate::identity::control::model::ActionRequest{connection:s.source.connection.clone(),operation:"order.update".into(),arguments:json!({"order":"PRIVATE-ORDER-42","address":"PRIVATE-EVENT-DESTINATION"})}).await.unwrap();
    let id = proposed.action["id"].as_str().unwrap();
    assert_eq!(
        s.db.approval_details(&s.owner, &s.w, id).await.unwrap()["source_scope"],
        s.source.id
    );
    assert!(
        s.db.control_snapshot(&s.owner, &s.w, &s.a)
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE-EVENT-DESTINATION")
    );
    s.db.control_rule(
        &s.owner,
        &s.w,
        &s.a,
        &rule(&s.source.connection, &s.c, "block", 1),
    )
    .await
    .unwrap();
    assert!(s.db.approval_details(&s.owner, &s.w, id).await.is_err());
    assert!(
        s.db.approval_inbox(&s.owner, &s.w).await.unwrap()["actions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        !s.db
            .control_snapshot(&s.owner, &s.w, &s.a)
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE-EVENT-DESTINATION")
    );
    assert!(
        s.db.control_decide(
            &s.owner,
            &s.w,
            &s.a,
            id,
            &super::control_tests::decision("approve")
        )
        .await
        .is_err()
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_archive_preserves_accepted_queued_task_but_stops_future_intake_and_restoration_does_not_resume()
 {
    let s = setup().await;
    s.save().await;
    s.accept("accepted", "delayed").await;
    s.db.dispatch_events().await.unwrap();
    let job = s.job().await;
    s.accept("pending", "delayed").await;
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
    let tx = s.db.read_tx(&s.w).await.unwrap();
    assert!(super::source_access::allowed(&tx, &job).await.unwrap());
    tx.commit().await.unwrap();
    assert_eq!(
        query(
            &s.db.db,
            "SELECT state FROM relay_chat_jobs WHERE reply=$1",
            vec![job.reply.into()]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "state")
        .unwrap(),
        "queued"
    );
    s.db.channel_lifecycle(
        &s.owner,
        &s.w,
        &s.c,
        Some(&crate::identity::chat::organization_model::Lifecycle {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            archived: false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    assert_eq!(
        s.db.event_trigger_details(
            &s.owner,
            &s.w,
            &s.a,
            &s.input.id,
            std::slice::from_ref(&s.source)
        )
        .await
        .unwrap()["state"],
        "blocked"
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_owner_role_does_not_expose_private_agents_or_destinations_and_account_forgery_never_saves()
 {
    let s = setup().await;
    s.save().await;
    let other = user(&s.db, "event-other-owner").await;
    join(&s.db, &s.owner, &s.w, &other, "User").await;
    s.db.change(&s.owner, &s.w, &other.id, Some("Owner"))
        .await
        .unwrap();
    assert!(
        s.db.event_trigger_details(
            &other,
            &s.w,
            &s.a,
            &s.input.id,
            std::slice::from_ref(&s.source)
        )
        .await
        .is_err()
    );
    let mut forged = payload("forged", "delayed");
    forged["metadata"]["connected_account_id"] = json!("other-account");
    assert!(
        s.db.accept_event(&s.source, "forged", &forged, 100000)
            .await
            .is_err()
    );
    assert!(
        query(&s.db.db, "SELECT id FROM relay_event_inbox", vec![])
            .await
            .unwrap()
            .is_none()
    );
    let foreign = workspace(&s.db, &other).await;
    assert!(
        s.db.event_triggers(&s.owner, &foreign, &s.a, &[])
            .await
            .is_err()
    );
    execute(
        &s.db.db,
        "UPDATE relay_shared_agents SET scope='private' WHERE workspace=$1 AND id=$2",
        vec![s.w.clone().into(), s.a.clone().into()],
    )
    .await
    .unwrap();
    assert!(
        s.db.event_triggers(&other, &s.w, &s.a, std::slice::from_ref(&s.source))
            .await
            .is_err()
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_operator_projection_rebinding_denies_old_sources_without_exposing_other_payload_fields()
 {
    let s = setup().await;
    s.save().await;
    s.accept("old-binding", "delayed").await;
    s.db.dispatch_events().await.unwrap();
    let job = s.job().await;
    let mut changed = s.source.clone();
    changed.fields = vec!["status".into()];
    crate::identity::events::test_catalog::register(changed).unwrap();
    assert!(s.db.event_job_context(&job).await.is_err());
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    let tx = s.db.workspace_tx(&s.w).await.unwrap();
    super::task_revocation::inaccessible(&tx, &s.w)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    crate::identity::events::test_catalog::register(s.source.clone()).unwrap();
    assert!(s.db.event_job_context(&job).await.is_err());
}
