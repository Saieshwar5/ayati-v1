use super::{
    control_tests::{request, setup},
    tests::*,
    *,
};
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agent_management_does_not_reveal_private_requests_or_their_counts() {
    let (db, owner, w, _, job) = setup("ask").await;
    let other = user(&db, "request-other-owner").await;
    join(&db, &owner, &w, &other, "Member").await;
    db.change(&owner, &w, &other.id, Some("Admin"))
        .await
        .unwrap();
    assert!(db.managed_agent(&other, &w, &job.agent, true).await.is_ok());
    assert!(
        db.agent_requests(&other, &w, &job.agent, false, false, None)
            .await
            .unwrap()["requests"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        db.agent_request(&other, &w, &job.agent, &job.reply)
            .await
            .is_err()
    );
    assert_eq!(
        db.agent_profiles(&other, &w).await.unwrap()[0]["pending"],
        0
    );
    let mine = db
        .agent_requests(&owner, &w, &job.agent, true, true, None)
        .await
        .unwrap();
    assert_eq!(mine["requests"][0]["id"], job.reply);
    assert_eq!(mine["requests"][0]["summary"], "Update the sample order");
    assert_eq!(mine["requests"][0]["state"], "running");
    assert_eq!(mine["requests"][0]["can_cancel"], true);
    db.channel_members(&owner, &w, &job.chat, &[owner.id.clone(), other.id.clone()])
        .await
        .unwrap();
    assert_eq!(
        db.agent_requests(&other, &w, &job.agent, true, true, None)
            .await
            .unwrap()["requests"],
        json!([])
    );
    let visible = db
        .agent_requests(&other, &w, &job.agent, false, true, None)
        .await
        .unwrap();
    assert_eq!(visible["requests"][0]["id"], job.reply);
    assert_eq!(visible["requests"][0]["can_cancel"], true);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn visible_work_is_not_permission_to_cancel_another_persons_request() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let viewer = user(&db, "request-visible-member").await;
    join(&db, &owner, &w, &viewer, "Member").await;
    db.chat_grant(&owner, &w, &viewer.id, &job.agent, true)
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
    let action = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap()
        .action;
    db.chat_finish(&job, "Waiting to update one order", "complete")
        .await
        .unwrap();
    let view = db
        .agent_request(&viewer, &w, &job.agent, &job.reply)
        .await
        .unwrap();
    assert_eq!(view["state"], "approval");
    assert_eq!(view["approvals"][0]["id"], action["id"]);
    assert_eq!(view["can_cancel"], false);
    assert!(
        db.cancel_assignment(&viewer, &w, &job.chat, &job.reply)
            .await
            .is_err()
    );
    db.cancel_assignment(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    db.cancel_assignment(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    let cancelled = db
        .agent_request(&owner, &w, &job.agent, &job.reply)
        .await
        .unwrap();
    assert_eq!(cancelled["state"], "cancelled");
    assert_eq!(cancelled["can_cancel"], false);
    assert!(cancelled["cancellation"]["by"].is_string());
    assert!(
        db.agent_requests(&owner, &w, &job.agent, true, true, None)
            .await
            .unwrap()["requests"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    db.chat_grant(&owner, &w, &viewer.id, &job.agent, false)
        .await
        .unwrap();
    assert!(
        db.agent_request(&viewer, &w, &job.agent, &job.reply)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn owner_can_stop_visible_work_after_requester_execution_access_is_revoked() {
    let (db, owner, w, _, first) = setup("ask").await;
    db.chat_finish(&first, "Done", "complete").await.unwrap();
    let requester = user(&db, "revoked-requester").await;
    join(&db, &owner, &w, &requester, "Member").await;
    db.chat_grant(&owner, &w, &requester.id, &first.agent, true)
        .await
        .unwrap();
    db.channel_members(
        &owner,
        &w,
        &first.chat,
        &[owner.id.clone(), requester.id.clone()],
    )
    .await
    .unwrap();
    db.chat_send(
        &requester,
        &w,
        &first.chat,
        &message("Prepare an update", Some(&first.agent)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_grant(&owner, &w, &requester.id, &job.agent, false)
        .await
        .unwrap();
    let request = db
        .agent_request(&owner, &w, &job.agent, &job.reply)
        .await
        .unwrap();
    assert_eq!(request["state"], "access_changed");
    assert_eq!(request["can_cancel"], true);
    db.cancel_assignment(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    assert_eq!(
        db.agent_request(&owner, &w, &job.agent, &job.reply)
            .await
            .unwrap()["state"],
        "cancelled"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn request_pages_preserve_original_source_and_thread_with_stable_cursors() {
    let (db, owner, w, _, first) = setup("ask").await;
    db.chat_finish(&first, "Done", "complete").await.unwrap();
    let root = message("Thread root", None);
    db.chat_send(&owner, &w, &first.chat, &root).await.unwrap();
    for i in 0..30 {
        let mut input = message(&format!("Request {i}"), Some(&first.agent));
        input.thread = Some(root.id.clone());
        db.chat_send(&owner, &w, &first.chat, &input).await.unwrap();
    }
    let page = db
        .agent_requests(&owner, &w, &first.agent, true, true, None)
        .await
        .unwrap();
    let records = page["requests"].as_array().unwrap();
    assert_eq!(records.len(), 25);
    assert_eq!(records[0]["summary"], "Request 29");
    assert_eq!(records[0]["thread"], root.id);
    assert!(records[0]["source_message"].is_string());
    assert_eq!(records[0]["state"], "queued");
    let next = db
        .agent_requests(
            &owner,
            &w,
            &first.agent,
            true,
            true,
            page["next_before"].as_i64(),
        )
        .await
        .unwrap();
    assert_eq!(next["requests"].as_array().unwrap().len(), 5);
    assert_eq!(next["requests"][0]["summary"], "Request 4");
    assert!(next["next_before"].is_null());
    assert!(
        !next["requests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| records.iter().any(|old| old["id"] == r["id"]))
    );
    assert!(
        db.agent_requests(&owner, &w, &first.agent, true, true, Some(-1))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn request_progress_rechecks_source_scope_and_does_not_mutate_execution() {
    let (db, owner, w, _, job) = setup("ask").await;
    let before = query(
        &db.db,
        "SELECT to_jsonb(j) AS data FROM relay_chat_jobs j WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<Value>("", "data")
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET progress=$2 WHERE id=$1",
        vec![
            job.reply.clone().into(),
            json!({"state":"responding","text":"Completed the comparison; checking totals."})
                .into(),
        ],
    )
    .await
    .unwrap();
    for _ in 0..3 {
        assert_eq!(
            db.agent_request(&owner, &w, &job.agent, &job.reply)
                .await
                .unwrap()["latest_update"],
            "Completed the comparison; checking totals."
        );
    }
    let after = query(
        &db.db,
        "SELECT to_jsonb(j) AS data FROM relay_chat_jobs j WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<Value>("", "data")
    .unwrap();
    assert_eq!(before, after);
    let other = workspace(&db, &owner).await;
    assert!(
        db.agent_request(&owner, &other, &job.agent, &job.reply)
            .await
            .is_err()
    );
    execute(
        &db.db,
        "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
        vec![
            w.clone().into(),
            job.chat.clone().into(),
            owner.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert!(
        db.agent_request(&owner, &w, &job.agent, &job.reply)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn focused_request_history_recovers_old_threads_and_rechecks_access() {
    let (db, owner, w, _, initial) = setup("ask").await;
    db.chat_finish(&initial, "Finished", "complete")
        .await
        .unwrap();
    let root = message("An old procurement discussion", None);
    db.chat_send(&owner, &w, &initial.chat, &root)
        .await
        .unwrap();
    let mut input = message("Check the old delivery request", Some(&initial.agent));
    input.thread = Some(root.id.clone());
    let requested = db
        .chat_send(&owner, &w, &initial.chat, &input)
        .await
        .unwrap()
        .unwrap();
    execute(&db.db,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) SELECT gen_random_uuid()::text,$1,$2,$3,'user','Later ordinary conversation','sent',1 FROM generate_series(1,220)",vec![w.clone().into(),initial.chat.clone().into(),owner.id.clone().into()]).await.unwrap();
    let recent = db.chat_messages(&owner, &w, &initial.chat).await.unwrap();
    assert!(
        !recent["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == requested.reply)
    );
    let focused = db
        .chat_focus(&owner, &w, &initial.chat, &requested.reply)
        .await
        .unwrap();
    let messages = focused["messages"].as_array().unwrap();
    assert!(
        messages
            .iter()
            .any(|m| m["id"] == requested.reply && m["thread"] == root.id)
    );
    assert!(messages.iter().any(|m| m["id"] == input.id));
    assert!(messages.iter().any(|m| m["id"] == root.id));
    let other = workspace(&db, &owner).await;
    assert!(
        db.chat_focus(&owner, &other, &initial.chat, &requested.reply)
            .await
            .is_err()
    );
    execute(
        &db.db,
        "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
        vec![
            w.clone().into(),
            initial.chat.clone().into(),
            owner.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert!(
        db.chat_focus(&owner, &w, &initial.chat, &requested.reply)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn library_source_revisions_guard_request_summaries_and_focused_reads() {
    use crate::identity::library::model::{AskItem, SavePage, ShareItem};
    let (db, owner, w, _, initial) = setup("ask").await;
    db.chat_finish(&initial, "Finished", "complete")
        .await
        .unwrap();
    let page = SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "Private policy".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"PRIVATE_POLICY_AMOUNT"}]}]}),
    };
    db.library_save(&owner, &w, &page).await.unwrap();
    let mut sharing = ShareItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        visibility: "private".into(),
        people: vec![],
        agents: vec![initial.agent.clone()],
    };
    db.library_share(&owner, &w, &page.id, &sharing)
        .await
        .unwrap();
    let result = db
        .library_ask(
            &owner,
            &w,
            &page.id,
            &AskItem {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                agent: initial.agent.clone(),
                question: "Review the private policy".into(),
            },
        )
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(job.chat, result["chat"]);
    assert!(
        db.agent_request(&owner, &w, &initial.agent, &job.reply)
            .await
            .is_ok()
    );
    assert!(
        db.chat_focus(&owner, &w, &job.chat, &job.reply)
            .await
            .is_ok()
    );
    sharing.request = uuid::Uuid::new_v4().to_string();
    sharing.revision = 2;
    sharing.agents.clear();
    db.library_share(&owner, &w, &page.id, &sharing)
        .await
        .unwrap();
    assert!(
        db.agent_request(&owner, &w, &initial.agent, &job.reply)
            .await
            .is_err()
    );
    assert!(
        db.chat_focus(&owner, &w, &job.chat, &job.reply)
            .await
            .is_err()
    );
    assert_eq!(
        db.agent_profiles(&owner, &w).await.unwrap()[0]["pending"],
        0
    );
    let visible = db
        .agent_requests(&owner, &w, &initial.agent, true, false, None)
        .await
        .unwrap();
    assert!(
        !visible["requests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == job.reply)
    );
}
