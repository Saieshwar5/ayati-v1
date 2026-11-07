//! Lookups by the asking message handle each agent's job: delete stops every
//! one, retry asks one agent again, receipts and instructions are per agent.
use super::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn delete_retry_and_receipts_handle_each_agents_job() {
    let r = room().await;
    let input = ask("count the boxes", &[("Atlas", &r.atlas), ("Nova", &r.nova)]);
    let jobs =
        r.db.chat_send_all(&r.member, &r.w, &r.c, &input)
            .await
            .unwrap();
    let (atlas, nova) = (&jobs[0], &jobs[1]);

    // Receipts are per agent: delivered to both, seen by the one that started.
    let tx = r.db.db.begin().await.unwrap();
    agent_receipts::seen(&tx, &r.w, &r.c, &input.id, &r.atlas)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mine = r.db.chat_messages(&r.member, &r.w, &r.c).await.unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == input.id)
        .cloned()
        .unwrap();
    assert_eq!(
        (mine["delivered"].as_i64(), mine["seen"].as_i64()),
        (Some(2), Some(1))
    );
    let details =
        r.db.receipt_details(&r.member, &r.w, &r.c, &input.id)
            .await
            .unwrap();
    let mut receipts: Vec<(String, bool)> = details["receipts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| (x["name"].as_str().unwrap().into(), x["seen_at"].is_i64()))
        .collect();
    receipts.sort();
    assert_eq!(
        receipts,
        [("Atlas".to_string(), true), ("Nova".to_string(), false)]
    );

    // Retrying Atlas's failed request asks only Atlas again; Nova's work goes on.
    execute(
        &r.db.db,
        "UPDATE relay_chat_jobs SET state='finished',lease_until=0 WHERE reply=$1",
        vec![atlas.reply.clone().into()],
    )
    .await
    .unwrap();
    execute(
        &r.db.db,
        "UPDATE relay_chat_messages SET state='failed' WHERE id=$1",
        vec![atlas.reply.clone().into()],
    )
    .await
    .unwrap();
    let retried =
        r.db.request_retry(&r.member, &r.w, &atlas.reply)
            .await
            .unwrap();
    let retry = retried["reply"].as_str().unwrap().to_string();
    let asked = query(&r.db.db, "SELECT j.payload->>'agent' AS agent,m.content,m.id FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.id=j.request_message WHERE j.reply=$1", vec![retry.clone().into()])
        .await.unwrap().unwrap();
    assert_eq!(asked.try_get::<String>("", "agent").unwrap(), r.atlas);
    assert_eq!(
        asked.try_get::<String>("", "content").unwrap(),
        format!("{} @Nova count the boxes", mention("Atlas", &r.atlas)),
        "Nova's mention became a plain name"
    );
    let retry_message: String = asked.try_get("", "id").unwrap();
    assert_eq!(r.jobs(&retry_message).await, 1);
    let again =
        r.db.request_retry(&r.member, &r.w, &atlas.reply)
            .await
            .unwrap();
    assert_eq!(again["reply"], retry.as_str(), "the same single retry");
    assert_eq!(
        r.state(&nova.reply).await,
        ("queued".into(), "queued".into())
    );

    // Deleting the message stops every agent's unfinished work.
    let both = ask(
        "and the pallets?",
        &[("Atlas", &r.atlas), ("Nova", &r.nova)],
    );
    let jobs =
        r.db.chat_send_all(&r.member, &r.w, &r.c, &both)
            .await
            .unwrap();
    assert_eq!(
        r.db.delete_message(&r.member, &r.w, &r.c, &both.id)
            .await
            .unwrap(),
        2
    );
    for job in &jobs {
        assert_eq!(
            r.state(&job.reply).await,
            ("finished".into(), "cancelled".into())
        );
    }
}

/// Claims a queued job as an executor would.
async fn claimed(db: &PgStore, job: &Job) -> Job {
    handoff_tests::claim(db, job).await
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn each_agent_is_told_who_else_was_asked_and_answers_from_the_same_evidence() {
    let r = room().await;
    let input = ask("stock?", &[("Atlas", &r.atlas), ("Nova", &r.nova)]);
    let jobs =
        r.db.chat_send_all(&r.member, &r.w, &r.c, &input)
            .await
            .unwrap();
    assert_eq!(r.db.asked_with(&jobs[0]).await.unwrap(), ["Nova"]);
    assert_eq!(r.db.asked_with(&jobs[1]).await.unwrap(), ["Atlas"]);

    // Atlas answers first; Nova's context still ends at the person's message.
    let atlas = claimed(&r.db, &jobs[0]).await;
    r.db.chat_finish(&atlas, "ATLAS_ANSWER twelve boxes", "complete")
        .await
        .unwrap();
    let nova = claimed(&r.db, &jobs[1]).await;
    let prepared = crate::identity::chat::execution::prepare(&r.db, &nova, 30)
        .await
        .unwrap();
    assert!(
        prepared
            .instructions
            .contains("Asked together: the same message also asked Atlas"),
        "{}",
        prepared.instructions
    );
    assert!(prepared.request.contains(&input.id));
    assert!(
        !prepared.request.contains("ATLAS_ANSWER"),
        "the other agent's answer is not evidence for the same message"
    );

    // A later message reads both the request and Atlas's answer as usual.
    let later = ask("and tomorrow?", &[("Nova", &r.nova)]);
    let job =
        r.db.chat_send(&r.member, &r.w, &r.c, &later)
            .await
            .unwrap()
            .unwrap();
    assert!(job.prompt.contains("ATLAS_ANSWER"));
    assert!(r.db.asked_with(&job).await.unwrap().is_empty());
    let single = claimed(&r.db, &job).await;
    let prepared = crate::identity::chat::execution::prepare(&r.db, &single, 30)
        .await
        .unwrap();
    assert!(!prepared.instructions.contains("Asked together"));
}
