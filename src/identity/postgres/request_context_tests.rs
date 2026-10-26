use super::{tests::*, *};
use serde_json::json;

fn evidence(prompt: &str) -> Value {
    serde_json::from_str(
        prompt
            .split("Conversation evidence (JSON):\n")
            .nth(1)
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn request_context_rebuilds_the_original_window_without_later_or_stale_snapshot_data() {
    let db = fixture().await;
    let owner = user(&db, "fresh-context-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let earlier = message("Earlier authorized conversation evidence", None);
    db.chat_send(&owner, &w, &c, &earlier).await.unwrap();
    let original = message("Answer this original request", Some(&a));
    let queued = db
        .chat_send(&owner, &w, &c, &original)
        .await
        .unwrap()
        .unwrap();
    // An upgrade may encounter a linked pre-attribution prompt. The original
    // request record, not that stale flattened text, determines fresh context.
    execute(&db.db,"UPDATE relay_chat_jobs SET payload=jsonb_set(payload,'{prompt}',to_jsonb($2::text)) WHERE reply=$1",vec![queued.reply.clone().into(),"STALE_UNATTRIBUTED_SOURCE_SNAPSHOT".into()]).await.unwrap();
    db.chat_send(&owner, &w, &c, &message("LATER_UNRELATED_MESSAGE", None))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.update_profile(&owner, "Current requester")
        .await
        .unwrap();
    let (identity, context) = db.request_context(&job).await.unwrap();
    assert_eq!(identity["origin"], "human_message");
    assert_eq!(identity["initiating_actor"]["id"], owner.id);
    assert_eq!(
        identity["initiating_actor"]["display_name"],
        "Current requester"
    );
    assert!(context.contains(&earlier.content));
    assert!(context.contains(&original.content));
    assert!(!context.contains("STALE_UNATTRIBUTED_SOURCE_SNAPSHOT"));
    assert!(!context.contains("LATER_UNRELATED_MESSAGE"));
    let value = evidence(&context);
    assert_eq!(value["request_message"], original.id);
    assert_eq!(value["messages"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["messages"][1]["author"]["display_name"],
        "Current requester"
    );
    // Rebuilding context does not rewrite durable task history or claim it again.
    let saved: Value = query(
        &db.db,
        "SELECT payload FROM relay_chat_jobs WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "payload")
    .unwrap();
    assert_eq!(saved["prompt"], "STALE_UNATTRIBUTED_SOURCE_SNAPSHOT");
    let mut stale = job.clone();
    stale.claim = Some("stale-claim".into());
    assert!(db.request_context(&stale).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn request_context_handoff_distinguishes_actual_agent_sender_from_human_principal() {
    let s = super::handoff_tests::setup().await;
    let (receipt, queued) = s.handoff().await;
    let request = query(
        &s.db.db,
        "SELECT request_message FROM relay_agent_handoffs WHERE workspace=$1 AND id=$2",
        vec![s.w.clone().into(), receipt["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<String>("", "request_message")
    .unwrap();
    let saved = evidence(&queued.prompt);
    let sender = saved["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == request)
        .unwrap();
    assert_eq!(sender["author"]["kind"], "agent");
    assert_eq!(sender["author"]["id"], s.a);
    assert_ne!(sender["author"]["id"], s.owner.id);
    s.db.chat_finish(&s.parent, "Waiting for helper", "complete")
        .await
        .unwrap();
    let child = super::handoff_tests::claim(&s.db, &queued).await;
    let (identity, prompt) = s.db.request_context(&child).await.unwrap();
    assert_eq!(identity["origin"], "agent_handoff");
    assert_eq!(identity["work_class"], "background");
    assert_eq!(identity["requesting_principal"]["id"], s.owner.id);
    assert_eq!(identity["initiating_actor"]["id"], s.a);
    assert_eq!(identity["initiating_actor"]["kind"], "agent");
    assert_eq!(identity["request_message"], request);
    let fresh = evidence(&prompt);
    let explicit = fresh["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == request)
        .unwrap();
    assert_eq!(explicit["author"]["id"], s.a);
    assert!(
        explicit["content"]
            .as_str()
            .unwrap()
            .contains("Check the relevant business facts")
    );
    assert!(!prompt.contains("Waiting for helper"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn request_context_keeps_historical_unknown_origin_and_current_history_eligibility() {
    let db = fixture().await;
    let owner = user(&db, "fresh-context-legacy").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let first = message("BEFORE_JOIN_EVIDENCE", None);
    db.chat_send(&owner, &w, &c, &first).await.unwrap();
    execute(&db.db,"UPDATE relay_chat_agents SET joined_sequence=(SELECT sequence FROM relay_chat_messages WHERE id=$3) WHERE workspace=$1 AND chat=$2",vec![w.clone().into(),c.clone().into(),first.id.into()]).await.unwrap();
    db.chat_send(
        &owner,
        &w,
        &c,
        &message("Original current request", Some(&a)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    assert!(
        db.request_context(&job)
            .await
            .unwrap()
            .1
            .contains("BEFORE_JOIN_EVIDENCE")
    );
    // Production profile edits require idle work or Pause; this also exercises
    // defensive current-policy behavior if stored policy changes during recovery.
    execute(&db.db,"UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{history}','\"since_join\"') WHERE workspace=$1 AND id=$2",vec![w.clone().into(),a.into()]).await.unwrap();
    assert!(
        !db.request_context(&job)
            .await
            .unwrap()
            .1
            .contains("BEFORE_JOIN_EVIDENCE")
    );
    db.chat_finish(&job, "Completed current request", "complete")
        .await
        .unwrap();
    super::migration_fixture::legacy_request(
        &db,
        &owner,
        &w,
        &c,
        &job.agent,
        "Preserved historical request without an exact message link",
    )
    .await
    .unwrap();
    let legacy = db.claim_job().await.unwrap().unwrap();
    let (identity, prompt) = db.request_context(&legacy).await.unwrap();
    assert_eq!(identity["origin"], "historical_request");
    assert_eq!(identity["initiating_actor"], Value::Null);
    assert_eq!(prompt, legacy.prompt);
    assert!(identity["requesting_principal"].get("role").is_none());
    assert_eq!(
        serde_json::to_value(&identity["request_message"]).unwrap(),
        json!(null)
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn request_context_background_origin_is_not_a_fresh_human_message() {
    let mut db = fixture().await;
    db.pooled = true;
    let owner = user(&db, "fresh-context-background").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let input = message("Scheduled work · Review\nReview the current work", Some(&a));
    let queued = db
        .chat_background(&owner, &w, &c, &input)
        .await
        .unwrap()
        .unwrap();
    let task = super::handoff_tests::claim(&db, &queued).await;
    let (identity, context) = db.request_context(&task).await.unwrap();
    assert_eq!(identity["requesting_principal"]["id"], owner.id);
    assert_eq!(identity["origin"], "background_automation");
    assert_eq!(identity["initiating_actor"], Value::Null);
    assert_eq!(identity["request_message"], input.id);
    assert!(context.contains("Review the current work"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn request_context_includes_an_earlier_answer_that_finished_while_the_next_request_waited() {
    let db = fixture().await;
    let owner = user(&db, "fresh-context-queued-answer").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_send(
        &owner,
        &w,
        &c,
        &message("Review our earlier decision", Some(&a)),
    )
    .await
    .unwrap();
    let first = db.claim_job().await.unwrap().unwrap();
    let second = db
        .chat_send(
            &owner,
            &w,
            &c,
            &message("Use that answer when you finish", Some(&a)),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        !second
            .prompt
            .contains("EARLIER_TASK_COMPLETED_WHILE_QUEUED")
    );
    db.chat_finish(&first, "EARLIER_TASK_COMPLETED_WHILE_QUEUED", "complete")
        .await
        .unwrap();
    let second = db.claim_job().await.unwrap().unwrap();
    let (_, context) = db.request_context(&second).await.unwrap();
    let value = evidence(&context);
    let answer = value["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == first.reply)
        .unwrap();
    assert_eq!(answer["author"]["kind"], "agent");
    assert_eq!(answer["author"]["id"], a);
    assert_eq!(answer["content"], "EARLIER_TASK_COMPLETED_WHILE_QUEUED");
}
