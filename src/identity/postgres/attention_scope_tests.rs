use super::{
    attention_tests::{ack, ask, page},
    control_tests::{request, setup},
    tests::*,
    *,
};
use crate::identity::{
    attention::Page,
    control::{approval_model::ApproverGrant, model::Rule},
    library::model::{AskItem, SavePage, ShareItem},
};
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn attention_designated_approver_needs_source_but_not_agent_request_authority() {
    let (db, owner, w, connection, job) = setup("ask").await;
    db.chat_finish(&job, "Initial work done", "complete")
        .await
        .unwrap();
    let approver = user(&db, "attention-approver").await;
    join(&db, &owner, &w, &approver, "User").await;
    db.channel_members(
        &owner,
        &w,
        &job.chat,
        &[owner.id.clone(), approver.id.clone()],
    )
    .await
    .unwrap();
    db.control_approver_grant(
        &owner,
        &w,
        &ApproverGrant {
            user: approver.id.clone(),
            connection: connection.clone(),
            operation: "order.update".into(),
            enabled: true,
            revision: 0,
            connection_revision: 1,
        },
    )
    .await
    .unwrap();
    db.control_rule(
        &owner,
        &w,
        &job.agent,
        &Rule {
            approver: Some(approver.id.clone()),
            approver_revision: Some(1),
            approval_chat: None,
            delegation_revision: None,
            connection: connection.clone(),
            operation: "order.update".into(),
            mode: "ask".into(),
            revision: 1,
            chat: job.chat.clone(),
        },
    )
    .await
    .unwrap();
    db.chat_send(
        &owner,
        &w,
        &job.chat,
        &message("Request for a designated person", Some(&job.agent)),
    )
    .await
    .unwrap();
    let active = db.claim_job().await.unwrap().unwrap();
    db.control_propose(&active, &request(&connection, "PRIVATE_APPROVAL_ARGUMENT"))
        .await
        .unwrap();
    db.chat_finish(&active, "Approval needed", "complete")
        .await
        .unwrap();
    assert!(!db.agent_access(&approver.id, &w, &job.agent).await.unwrap());
    let approved_queue = db.attention(&approver, &w, &Page::default()).await.unwrap();
    assert_eq!(approved_queue["items"].as_array().unwrap().len(), 1);
    assert!(
        !approved_queue
            .to_string()
            .contains("PRIVATE_APPROVAL_ARGUMENT")
    );
    let item = &approved_queue["items"][0];
    assert!(
        db.acknowledge_attention(&owner, &w, &ack(item, false))
            .await
            .is_err()
    );
    assert!(
        db.attention(&owner, &w, &Page::default()).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    db.acknowledge_attention(&approver, &w, &ack(item, false))
        .await
        .unwrap();
    assert_eq!(
        db.approval_details(&approver, &w, item["id"].as_str().unwrap())
            .await
            .unwrap()["state"],
        "pending"
    );
    db.channel_members(&owner, &w, &job.chat, std::slice::from_ref(&owner.id))
        .await
        .unwrap();
    let hidden = db.attention(&approver, &w, &Page::default()).await.unwrap();
    assert_eq!(hidden["items"], json!([]));
    assert!(hidden["next_before"].is_null());
    assert!(
        db.acknowledge_attention(&approver, &w, &ack(item, false))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn attention_filters_library_sources_before_pagination_and_rejects_stale_acknowledgments() {
    let (db, owner, w, _, initial) = setup("ask").await;
    db.finish_claimed_job(&initial, "Ordinary result", "complete")
        .await
        .unwrap();
    let source = SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "PRIVATE_LIBRARY_SOURCE".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Private reference"}]}]}),
    };
    db.library_save(&owner, &w, &source).await.unwrap();
    let mut sharing = ShareItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        visibility: "private".into(),
        people: vec![],
        agents: vec![initial.agent.clone()],
    };
    db.library_share(&owner, &w, &source.id, &sharing)
        .await
        .unwrap();
    let mut question_id = String::new();
    // More than a complete page of newer, subsequently denied records must not
    // hide the older accessible ordinary result or produce a hidden cursor.
    for n in 0..27 {
        db.library_ask(
            &owner,
            &w,
            &source.id,
            &AskItem {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                agent: initial.agent.clone(),
                question: format!("PRIVATE_LIBRARY_QUESTION_{n}"),
            },
        )
        .await
        .unwrap();
        let job = db.claim_job().await.unwrap().unwrap();
        if n == 26 {
            question_id = db.ask_question(&job, &ask()).await.unwrap()["id"]
                .as_str()
                .unwrap()
                .into();
            db.chat_finish(&job, "Waiting", "complete").await.unwrap();
        } else {
            db.finish_claimed_job(&job, "Saved private result", "complete")
                .await
                .unwrap();
        }
    }
    let recent = db.attention(&owner, &w, &page("recent")).await.unwrap();
    assert_eq!(recent["items"].as_array().unwrap().len(), 25);
    assert!(recent["next_before"].is_string());
    let saved = recent["items"][0].clone();
    db.acknowledge_attention(&owner, &w, &ack(&saved, false))
        .await
        .unwrap();
    assert_eq!(
        db.attention(&owner, &w, &Page::default()).await.unwrap()["items"][0]["kind"],
        "question"
    );
    sharing.request = uuid::Uuid::new_v4().to_string();
    sharing.revision = 2;
    sharing.agents.clear();
    db.library_share(&owner, &w, &source.id, &sharing)
        .await
        .unwrap();
    let filtered = db.attention(&owner, &w, &page("recent")).await.unwrap();
    assert_eq!(filtered["items"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["items"][0]["id"], initial.reply);
    assert!(filtered["next_before"].is_null());
    assert!(!filtered.to_string().contains("PRIVATE_LIBRARY"));
    assert!(
        db.acknowledge_attention(&owner, &w, &ack(&saved, false))
            .await
            .is_err()
    );
    assert!(
        db.attention(&owner, &w, &Page::default()).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(db.claim_job().await.unwrap().is_none());
    assert!(db.question(&owner, &w, &question_id).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn attention_cursor_ties_do_not_skip_or_repeat_work_and_foreign_ids_do_not_acknowledge() {
    let (db, owner, w, _, initial) = setup("ask").await;
    db.finish_claimed_job(&initial, "First result", "complete")
        .await
        .unwrap();
    let mut expected = std::collections::HashSet::from([initial.reply.clone()]);
    for n in 0..52 {
        db.chat_send(
            &owner,
            &w,
            &initial.chat,
            &message(&format!("Result {n}"), Some(&initial.agent)),
        )
        .await
        .unwrap();
        let job = db.claim_job().await.unwrap().unwrap();
        db.finish_claimed_job(&job, "Done", "complete")
            .await
            .unwrap();
        expected.insert(job.reply);
    }
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET finished_ms=1000 WHERE workspace=$1 AND state='finished'",
        vec![w.clone().into()],
    )
    .await
    .unwrap();
    let other = workspace(&db, &owner).await;
    let mut input = page("recent");
    let mut seen = std::collections::HashSet::new();
    let mut lengths = vec![];
    loop {
        let next = db.attention(&owner, &w, &input).await.unwrap();
        lengths.push(next["items"].as_array().unwrap().len());
        for item in next["items"].as_array().unwrap() {
            assert!(seen.insert(item["id"].as_str().unwrap().to_owned()));
            assert!(
                db.acknowledge_attention(&owner, &other, &ack(item, false))
                    .await
                    .is_err()
            );
        }
        input.before = next["next_before"].as_str().map(str::to_owned);
        if input.before.is_none() {
            break;
        }
    }
    assert_eq!(lengths, vec![25, 25, 3]);
    assert_eq!(seen, expected);
    input.before = Some("invalid".into());
    assert!(db.attention(&owner, &w, &input).await.is_err());
    let stranger = user(&db, "attention-stranger").await;
    assert!(db.attention(&stranger, &w, &Page::default()).await.is_err());
}
