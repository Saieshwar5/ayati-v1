use super::{
    control_tests::{request, setup},
    tests::*,
    *,
};
use serde_json::json;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn task_progress_read_visibility_does_not_grant_or_require_request_permission() {
    let (db, owner, w, _, job) = setup("ask").await;
    let reader = user(&db, "progress-reader").await;
    join(&db, &owner, &w, &reader, "Member").await;
    assert!(
        db.task_progress(&reader, &w, &job.chat, &job.reply)
            .await
            .is_err()
    );
    db.channel_members(
        &owner,
        &w,
        &job.chat,
        &[owner.id.clone(), reader.id.clone()],
    )
    .await
    .unwrap();
    assert!(!db.agent_access(&reader.id, &w, &job.agent).await.unwrap());
    assert!(
        db.agent_request(&reader, &w, &job.agent, &job.reply)
            .await
            .is_err()
    );
    let before: Value = query(
        &db.db,
        "SELECT to_jsonb(j) AS data FROM relay_chat_jobs j WHERE workspace=$1 AND reply=$2",
        vec![w.clone().into(), job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "data")
    .unwrap();
    for _ in 0..3 {
        let data = db
            .task_progress(&reader, &w, &job.chat, &job.reply)
            .await
            .unwrap();
        assert_eq!(data["state"], "running");
        assert_eq!(data["can_cancel"], false);
        assert_eq!(data["id"], job.reply);
        assert!(data["checked_ms"].as_i64().unwrap() > 0);
        assert_eq!(data["worker_released"], false);
        assert_eq!(data["questions"], json!([]));
    }
    let after: Value = query(
        &db.db,
        "SELECT to_jsonb(j) AS data FROM relay_chat_jobs j WHERE workspace=$1 AND reply=$2",
        vec![w.clone().into(), job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "data")
    .unwrap();
    assert_eq!(before, after);
    assert!(
        db.cancel_assignment(&reader, &w, &job.chat, &job.reply)
            .await
            .is_err()
    );
    assert!(
        db.task_progress(&reader, &w, &uuid::Uuid::new_v4().to_string(), &job.reply)
            .await
            .is_err()
    );
    let other = workspace(&db, &reader).await;
    assert!(
        db.task_progress(&reader, &other, &job.chat, &job.reply)
            .await
            .is_err()
    );
    db.channel_members(&owner, &w, &job.chat, std::slice::from_ref(&owner.id))
        .await
        .unwrap();
    assert!(
        db.task_progress(&reader, &w, &job.chat, &job.reply)
            .await
            .is_err()
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn task_progress_reports_saved_input_approval_recovery_and_terminal_outcomes() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let ask=serde_json::from_value(json!({"key":"delivery","specification":{"version":1,"kind":"single_choice","title":"Choose delivery","options":[{"id":"road","label":"Road"},{"id":"air","label":"Air"}]}})).unwrap();
    let question = db.ask_question(&job, &ask).await.unwrap();
    db.chat_finish(&job, "I need your delivery choice.", "complete")
        .await
        .unwrap();
    let data = db
        .task_progress(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    assert_eq!(data["state"], "input");
    assert_eq!(data["worker_released"], true);
    assert_eq!(data["questions"][0]["id"], question["id"]);
    assert_eq!(data["questions"][0]["respondent"], owner.id);
    db.answer_question(
        &owner,
        &w,
        question["id"].as_str().unwrap(),
        &crate::identity::interactions::model::Submission {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            decision: "answer".into(),
            values: json!({"choices":["road"]}),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.task_progress(&owner, &w, &job.chat, &job.reply)
            .await
            .unwrap()["state"],
        "resuming"
    );
    let resumed = db.claim_job().await.unwrap().unwrap();
    let action = db
        .control_propose(&resumed, &request(&connection, "progress-order"))
        .await
        .unwrap()
        .action;
    db.chat_finish(&resumed, "The change needs approval.", "complete")
        .await
        .unwrap();
    let data = db
        .task_progress(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    assert_eq!(data["state"], "approval");
    assert_eq!(data["approvals"][0]["id"], action["id"]);
    assert_eq!(data["worker_released"], true);
    db.cancel_assignment(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    let data = db
        .task_progress(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    assert_eq!(data["state"], "cancelled");
    assert_eq!(data["can_cancel"], false);
    assert!(data["finished_ms"].as_i64().unwrap() > 0);
    let queued = db
        .chat_send(
            &owner,
            &w,
            &job.chat,
            &message("Fresh request", Some(&job.agent)),
        )
        .await
        .unwrap();
    let queued = queued.unwrap();
    assert_eq!(
        db.task_progress(&owner, &w, &job.chat, &queued.reply)
            .await
            .unwrap()["state"],
        "queued"
    );
    let claimed = db.claim_job().await.unwrap().unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET lease_until=1 WHERE workspace=$1 AND reply=$2",
        vec![w.clone().into(), claimed.reply.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(
        db.task_progress(&owner, &w, &job.chat, &claimed.reply)
            .await
            .unwrap()["state"],
        "recovering"
    );
    // A status read never claims an expired lease or changes the task.
    assert_eq!(
        query(
            &db.db,
            "SELECT lease_until FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2",
            vec![w.clone().into(), claimed.reply.clone().into()]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "lease_until")
        .unwrap(),
        1
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn task_progress_preserves_private_agent_and_library_source_boundaries() {
    use crate::identity::library::model::{AskItem, SavePage, ShareItem};
    let (db, owner, w, _, job) = setup("ask").await;
    let viewer = user(&db, "progress-private-viewer").await;
    join(&db, &owner, &w, &viewer, "Member").await;
    db.change(&owner, &w, &viewer.id, Some("Admin"))
        .await
        .unwrap();
    db.channel_members(
        &owner,
        &w,
        &job.chat,
        &[owner.id.clone(), viewer.id.clone()],
    )
    .await
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET scope='private',owner_id=$3 WHERE workspace=$1 AND id=$2",
        vec![
            w.clone().into(),
            job.agent.clone().into(),
            owner.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert!(
        db.task_progress(&viewer, &w, &job.chat, &job.reply)
            .await
            .is_err()
    );
    assert!(
        db.task_progress(&owner, &w, &job.chat, &job.reply)
            .await
            .is_ok()
    );
    db.chat_finish(&job, "Finished", "complete").await.unwrap();
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET scope='workspace' WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), job.agent.clone().into()],
    )
    .await
    .unwrap();
    let page = SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "Private source".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"PROGRESS_SOURCE_SECRET"}]}]}),
    };
    db.library_save(&owner, &w, &page).await.unwrap();
    let mut share = ShareItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        visibility: "private".into(),
        people: vec![],
        agents: vec![job.agent.clone()],
    };
    db.library_share(&owner, &w, &page.id, &share)
        .await
        .unwrap();
    let receipt = db
        .library_ask(
            &owner,
            &w,
            &page.id,
            &AskItem {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                agent: job.agent.clone(),
                question: "Review the saved page".into(),
            },
        )
        .await
        .unwrap();
    let source_job = db.claim_job().await.unwrap().unwrap();
    let id = source_job.reply.as_str();
    let chat = receipt["chat"].as_str().unwrap();
    assert!(db.task_progress(&owner, &w, chat, id).await.is_ok());
    share.request = uuid::Uuid::new_v4().to_string();
    share.revision = 2;
    share.agents.clear();
    db.library_share(&owner, &w, &page.id, &share)
        .await
        .unwrap();
    assert!(db.task_progress(&owner, &w, chat, id).await.is_err());
}
