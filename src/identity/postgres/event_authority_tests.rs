use super::{event_tests::*, tests::*, *};
use serde_json::json;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_standing_read_requires_an_allowed_read_and_original_person_alone_resumes() {
    let mut s = setup().await;
    let member = user(&s.db, "event-delegated-author").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    s.db.channel_members(
        &s.owner,
        &s.w,
        &s.c,
        &[s.owner.id.clone(), member.id.clone()],
    )
    .await
    .unwrap();
    execute(
        &s.db.db,
        "UPDATE relay_shared_agents SET owner_id=$3 WHERE workspace=$1 AND id=$2",
        vec![
            s.w.clone().into(),
            s.a.clone().into(),
            member.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    // The admin's workspace rule bounds every agent's standing reads.
    let rule = |mode: &'static str| {
        let (db, owner, w, conn) = (&s.db, &s.owner, s.w.clone(), s.source.connection.clone());
        async move {
            super::tool_test_support::workspace_rule(db, owner, &w, &conn, "order.read", mode).await
        }
    };
    rule("block").await;
    assert!(
        s.db.save_event_trigger(&member, &s.w, &s.a, &s.input, Some(&s.source))
            .await
            .is_err()
    );
    // Ask first is not enough for a standing read.
    rule("ask").await;
    assert!(
        s.db.event_triggers(&member, &s.w, &s.a, std::slice::from_ref(&s.source))
            .await
            .unwrap()["choices"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    rule("allow").await;
    let choices =
        s.db.event_triggers(&member, &s.w, &s.a, std::slice::from_ref(&s.source))
            .await
            .unwrap()["choices"]
            .clone();
    s.input.read_revision = choices[0]["authority"]["read_revision"].as_i64().unwrap();
    s.db.save_event_trigger(&member, &s.w, &s.a, &s.input, Some(&s.source))
        .await
        .unwrap();
    let paused =
        s.db.change_event_trigger(
            &s.owner,
            &s.w,
            &s.a,
            &s.input.id,
            &s.change(1, false),
            std::slice::from_ref(&s.source),
        )
        .await
        .unwrap();
    assert_eq!(paused["state"], "paused");
    let resume = s.change(2, true);
    assert!(
        s.db.change_event_trigger(
            &s.owner,
            &s.w,
            &s.a,
            &s.input.id,
            &resume,
            std::slice::from_ref(&s.source)
        )
        .await
        .is_err()
    );
    s.db.change_event_trigger(
        &member,
        &s.w,
        &s.a,
        &s.input.id,
        &resume,
        std::slice::from_ref(&s.source),
    )
    .await
    .unwrap();
    s.accept("delegated-event", "delayed").await;
    s.db.dispatch_events().await.unwrap();
    let job = s.job().await;
    assert_eq!(job.requester, member.id);
    rule("block").await;
    assert!(s.db.event_job_context(&job).await.is_err());
    rule("allow").await;
    assert!(s.db.event_job_context(&job).await.is_err());
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_exact_creation_receipt_survives_execution_mode_and_catalog_loss_without_reapplying_work()
 {
    let mut s = setup().await;
    let saved = s.save().await;
    s.db.pooled = false;
    let replay =
        s.db.save_event_trigger(&s.owner, &s.w, &s.a, &s.input, None)
            .await
            .unwrap();
    assert_eq!(replay["id"], saved["id"]);
    assert_eq!(replay["revision"], 1);
    let mut changed = s.input.clone();
    changed.instructions = "Different instructions".into();
    assert!(
        s.db.save_event_trigger(&s.owner, &s.w, &s.a, &changed, None)
            .await
            .is_err()
    );
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    assert_eq!(
        s.db.company_counts(&s.owner, &s.w).await.unwrap()["jobs"],
        0
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_key_rotation_preserves_source_identity_and_pending_delivery_but_semantic_rebinding_requires_new_trigger()
 {
    let s = setup().await;
    s.save().await;
    s.accept("rotation", "delayed").await;
    let mut rotated = s.source.clone();
    rotated.secret_file = "/rotated/elsewhere".into();
    assert_eq!(
        s.source.fingerprint().unwrap(),
        rotated.fingerprint().unwrap()
    );
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let mut changed = s.input.clone();
    changed.binding = "b".repeat(43);
    changed.id = uuid::Uuid::new_v4().to_string();
    assert!(
        s.db.save_event_trigger(&s.owner, &s.w, &s.a, &changed, Some(&s.source))
            .await
            .is_err()
    );
    let current =
        s.db.event_triggers(&s.owner, &s.w, &s.a, std::slice::from_ref(&s.source))
            .await
            .unwrap();
    assert_eq!(current["triggers"].as_array().unwrap().len(), 1);
    assert_eq!(
        current["triggers"][0]["source_definition"].get("secret_file"),
        None
    );
    assert_eq!(
        current["triggers"][0]["source_definition"].get("user_id"),
        None
    );
    assert!(json!(current).to_string().len() < 10000);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_queue_pressure_rotates_rules_without_allowing_a_later_delivery_to_overtake_its_head()
{
    let s = setup().await;
    s.save().await;
    let mut first = None;
    for _ in 0..32 {
        let job =
            s.db.chat_background(
                &s.owner,
                &s.w,
                &s.c,
                &message("Existing background work", Some(&s.a)),
            )
            .await
            .unwrap()
            .unwrap();
        if first.is_none() {
            first = Some(job.reply);
        }
    }
    s.accept("head-event", "delayed").await;
    s.accept("later-event", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    s.db.cancel_assignment(&s.owner, &s.w, &s.c, &first.unwrap())
        .await
        .unwrap();
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let accepted = query(
        &s.db.db,
        "SELECT event_id FROM relay_event_deliveries WHERE reply IS NOT NULL",
        vec![],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        accepted.try_get::<String>("", "event_id").unwrap(),
        "head-event"
    );
}
