use super::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn withdrawing_and_regranting_request_permission_cannot_revive_waiting_work() {
    let (db, owner, w, connection, initial) = setup("ask").await;
    db.chat_finish(&initial, "No operation needed", "complete")
        .await
        .unwrap();
    let requester = user(&db, "continuation-requester").await;
    join(&db, &owner, &w, &requester, "Member").await;
    db.chat_grant(&owner, &w, &requester.id, &initial.agent, true)
        .await
        .unwrap();
    db.channel_members(
        &owner,
        &w,
        &initial.chat,
        &[owner.id.clone(), requester.id.clone()],
    )
    .await
    .unwrap();
    db.chat_send(
        &requester,
        &w,
        &initial.chat,
        &message("Update an order", Some(&initial.agent)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let a = db
        .control_propose(&job, &request(&connection, "grant-revoked"))
        .await
        .unwrap()
        .action;
    db.chat_finish(&job, "Waiting for approval", "complete")
        .await
        .unwrap();
    db.chat_grant(&owner, &w, &requester.id, &job.agent, false)
        .await
        .unwrap();
    db.chat_grant(&owner, &w, &requester.id, &job.agent, true)
        .await
        .unwrap();
    assert_eq!(state(&db, &job).await["state"], "finished");
    assert!(
        db.control_decide(
            &owner,
            &w,
            &job.agent,
            a["id"].as_str().unwrap(),
            &decision("approve")
        )
        .await
        .is_err()
    );
    assert!(db.claim_job().await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn lost_workers_recover_the_same_proposal_and_checkpoint_without_repeating_effects() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let a = db
        .control_propose(&job, &request(&connection, "crash"))
        .await
        .unwrap()
        .action;
    db.release_job(&job).await.unwrap();
    let replacement = db.claim_job().await.unwrap().unwrap();
    assert_eq!(
        db.control_propose(&replacement, &request(&connection, "crash"))
            .await
            .unwrap()
            .action["id"],
        a["id"]
    );
    assert!(
        !db.finish_claimed_job(&job, "Old runtime", "complete")
            .await
            .unwrap()
    );
    db.chat_finish(
        &replacement,
        "Recovered proposal; finish summary after approval",
        "complete",
    )
    .await
    .unwrap();
    complete_action(&db, &owner, &w, &job, a["id"].as_str().unwrap()).await;
    let first_resume = db.claim_job().await.unwrap().unwrap();
    let saved = db.task_continuation(&first_resume).await.unwrap().unwrap();
    db.release_job(&first_resume).await.unwrap();
    let recovered_resume = db.claim_job().await.unwrap().unwrap();
    assert_eq!(
        db.task_continuation(&recovered_resume)
            .await
            .unwrap()
            .unwrap(),
        saved
    );
    assert!(
        db.control_propose(&recovered_resume, &request(&connection, "crash"))
            .await
            .unwrap()
            .dispatch
            .is_none()
    );
    db.chat_finish(&recovered_resume, "Recovered original task", "complete")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn removing_and_readding_source_membership_never_revives_a_saved_task() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let manager = user(&db, "source-manager").await;
    join(&db, &owner, &w, &manager, "Member").await;
    db.change(&owner, &w, &manager.id, Some("Admin"))
        .await
        .unwrap();
    db.channel_members(
        &owner,
        &w,
        &job.chat,
        &[owner.id.clone(), manager.id.clone()],
    )
    .await
    .unwrap();
    let a = db
        .control_propose(&job, &request(&connection, "revoked-source"))
        .await
        .unwrap()
        .action;
    db.chat_finish(&job, "Private saved task", "complete")
        .await
        .unwrap();
    db.channel_members(&manager, &w, &job.chat, std::slice::from_ref(&manager.id))
        .await
        .unwrap();
    db.channel_members(
        &manager,
        &w,
        &job.chat,
        &[manager.id.clone(), owner.id.clone()],
    )
    .await
    .unwrap();
    assert_eq!(state(&db, &job).await["state"], "finished");
    assert_eq!(
        super::approval_delivery::action(&db.db, &w, a["id"].as_str().unwrap())
            .await
            .unwrap()["state"],
        "invalidated"
    );
    assert!(
        db.control_decide(
            &owner,
            &w,
            &job.agent,
            a["id"].as_str().unwrap(),
            &decision("approve")
        )
        .await
        .is_err()
    );
    assert!(db.claim_job().await.unwrap().is_none());
}
