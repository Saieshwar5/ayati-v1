use super::{tests::*, *};
use crate::identity::{
    chat::store::Job,
    control::model::{ActionRequest, Decision, Rule},
};
use serde_json::json;

async fn setup(mode: &str) -> (PgStore, User, String, String, Job) {
    let db = fixture().await;
    let owner = user(&db, "control-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let connection = uuid::Uuid::new_v4().to_string();
    let definition = json!({"id":"orders","name":"Orders","operations":[{"id":"order.update","title":"Update order","tool":"update_order","effect":"write","schema":{"type":"object","properties":{"order":{"type":"string"},"address":{"type":"string"}},"required":["order","address"],"additionalProperties":false}}]});
    db.control_connect(&owner, &w, &agent, &connection, definition, "reviewed")
        .await
        .unwrap();
    db.control_rule(
        &owner,
        &w,
        &agent,
        &Rule {
            connection: connection.clone(),
            operation: "order.update".into(),
            mode: mode.into(),
            revision: 0,
            chat: chat.clone(),
        },
    )
    .await
    .unwrap();
    db.chat_send(
        &owner,
        &w,
        &chat,
        &message("Update the sample order", Some(&agent)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    (db, owner, w, connection, job)
}
fn request(connection: &str, order: &str) -> ActionRequest {
    ActionRequest {
        connection: connection.into(),
        operation: "order.update".into(),
        arguments: json!({"order":order,"address":"Reviewed address"}),
    }
}
fn decision(value: &str) -> Decision {
    Decision {
        id: uuid::Uuid::new_v4().to_string(),
        decision: value.into(),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn control_exact_replay_and_unknown_write_hold_survive_new_model_request_ids() {
    let (db, owner, w, connection, job) = setup("allow").await;
    let first = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    let dispatch = first.dispatch.unwrap();
    assert_eq!(first.action["state"], "executing");
    let repeated = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    assert_eq!(repeated.action["id"], first.action["id"]);
    assert!(repeated.dispatch.is_none());
    db.control_finish(&w, &dispatch, json!({"message":"Lost response"}), false)
        .await
        .unwrap();
    let repeated = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    assert_eq!(repeated.action["state"], "uncertain");
    assert!(
        db.control_propose(&job, &request(&connection, "two"))
            .await
            .is_err()
    );
    let snapshot = db.control_snapshot(&owner, &w, &job.agent).await.unwrap();
    assert_eq!(snapshot["actions"].as_array().unwrap().len(), 1);
    assert!(snapshot["actions"][0].get("dispatch_token").is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn control_approval_is_exact_single_dispatch_and_works_after_proposal_finishes() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let proposal = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    assert!(proposal.dispatch.is_none());
    assert_eq!(proposal.action["state"], "pending");
    let id = proposal.action["id"].as_str().unwrap();
    assert!(
        db.finish_claimed_job(&job, "Approval needed", "complete")
            .await
            .unwrap()
    );
    let a = decision("approve");
    let b = decision("approve");
    let (first, second) = tokio::join!(
        db.control_decide(&owner, &w, &job.agent, id, &a),
        db.control_decide(&owner, &w, &job.agent, id, &b)
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(
        usize::from(first.dispatch.is_some()) + usize::from(second.dispatch.is_some()),
        1
    );
    let dispatch = first.dispatch.or(second.dispatch).unwrap();
    assert_eq!(
        dispatch.arguments,
        json!({"order":"one","address":"Reviewed address"})
    );
    db.control_finish(&w, &dispatch, json!({"confirmed":true}), true)
        .await
        .unwrap();
    let retry = db
        .control_decide(&owner, &w, &job.agent, id, &a)
        .await
        .unwrap();
    assert_eq!(retry.action["state"], "completed");
    assert!(retry.dispatch.is_none());
    assert!(
        db.control_decide(&owner, &w, &job.agent, id, &decision("reject"))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn control_owner_rules_revoke_pending_approval_without_reviving_it() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let member = user(&db, "control-member").await;
    join(&db, &owner, &w, &member, "User").await;
    let proposed = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    let id = proposed.action["id"].as_str().unwrap();
    assert!(
        db.control_decide(&member, &w, &job.agent, id, &decision("approve"))
            .await
            .is_err()
    );
    let rule = Rule {
        connection: connection.clone(),
        operation: "order.update".into(),
        mode: "block".into(),
        revision: 1,
        chat: job.chat.clone(),
    };
    assert!(
        db.control_rule(&member, &w, &job.agent, &rule)
            .await
            .is_err()
    );
    db.control_rule(&owner, &w, &job.agent, &rule)
        .await
        .unwrap();
    // Exact retry does not advance the revision twice.
    db.control_rule(&owner, &w, &job.agent, &rule)
        .await
        .unwrap();
    assert!(
        db.control_decide(&owner, &w, &job.agent, id, &decision("approve"))
            .await
            .is_err()
    );
    let blocked = db
        .control_propose(&job, &request(&connection, "two"))
        .await
        .unwrap();
    assert_eq!(blocked.action["state"], "denied");
    assert!(blocked.dispatch.is_none());
    db.control_rule(
        &owner,
        &w,
        &job.agent,
        &Rule {
            revision: 2,
            mode: "ask".into(),
            ..rule
        },
    )
    .await
    .unwrap();
    assert!(
        db.control_decide(&owner, &w, &job.agent, id, &decision("approve"))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn control_scope_ownership_visibility_and_disconnection_are_server_enforced() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let proposed = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    let other = workspace(&db, &owner).await;
    assert!(
        db.control_snapshot(&owner, &other, &job.agent)
            .await
            .is_err()
    );
    let mut wrong = job.clone();
    wrong.workspace = other;
    assert!(
        db.control_propose(&wrong, &request(&connection, "two"))
            .await
            .is_err()
    );
    wrong = job.clone();
    wrong.claim = Some(uuid::Uuid::new_v4().to_string());
    assert!(db.control_operations(&wrong).await.is_err());
    wrong = job.clone();
    wrong.chat = chat(&db, &owner, &w, vec![], vec![job.agent.clone()]).await;
    assert!(
        db.control_propose(&wrong, &request(&connection, "two"))
            .await
            .is_err()
    );
    db.control_disconnect(&owner, &w, &job.agent, &connection)
        .await
        .unwrap();
    assert!(
        db.control_propose(&job, &request(&connection, "two"))
            .await
            .is_err()
    );
    assert!(
        db.control_decide(
            &owner,
            &w,
            &job.agent,
            proposed.action["id"].as_str().unwrap(),
            &decision("approve")
        )
        .await
        .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn control_lost_channel_access_hides_receipts_and_prevents_approval() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let proposal = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
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
    assert!(db.control_operations(&job).await.is_err());
    let snapshot = db.control_snapshot(&owner, &w, &job.agent).await.unwrap();
    assert!(snapshot["actions"].as_array().unwrap().is_empty());
    assert!(
        db.control_decide(
            &owner,
            &w,
            &job.agent,
            proposal.action["id"].as_str().unwrap(),
            &decision("approve")
        )
        .await
        .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn control_conversation_scope_and_recheck_fence_revocation_during_setup() {
    let (db, owner, w, connection, job) = setup("allow").await;
    let proposal = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    let dispatch = proposal.dispatch.unwrap();
    db.control_ready(&w, &dispatch).await.unwrap();
    db.control_rule(
        &owner,
        &w,
        &job.agent,
        &Rule {
            connection: connection.clone(),
            operation: "order.update".into(),
            mode: "block".into(),
            revision: 1,
            chat: job.chat.clone(),
        },
    )
    .await
    .unwrap();
    assert!(db.control_ready(&w, &dispatch).await.is_err());
    assert_eq!(
        db.control_preflight_failed(&w, &dispatch).await.unwrap()["state"],
        "invalidated"
    );
    db.control_rule(
        &owner,
        &w,
        &job.agent,
        &Rule {
            connection: connection.clone(),
            operation: "order.update".into(),
            mode: "allow".into(),
            revision: 2,
            chat: job.chat.clone(),
        },
    )
    .await
    .unwrap();
    db.finish_claimed_job(&job, "Finished", "complete")
        .await
        .unwrap();
    let other = chat(&db, &owner, &w, vec![], vec![job.agent.clone()]).await;
    db.chat_send(
        &owner,
        &w,
        &other,
        &message("Try from another conversation", Some(&job.agent)),
    )
    .await
    .unwrap();
    let other_job = db.claim_job().await.unwrap().unwrap();
    assert!(
        db.control_operations(&other_job).await.unwrap()["operations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.control_propose(&other_job, &request(&connection, "two"))
            .await
            .unwrap()
            .action["state"],
        "denied"
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn control_connection_review_never_inherits_previous_operation_grants() {
    let (db, owner, w, connection, job) = setup("allow").await;
    let snapshot = db.control_snapshot(&owner, &w, &job.agent).await.unwrap();
    db.control_connect(
        &owner,
        &w,
        &job.agent,
        &connection,
        snapshot["connections"][0]["definition"].clone(),
        "new-credential-or-operation-revision",
    )
    .await
    .unwrap();
    assert_eq!(
        db.control_propose(&job, &request(&connection, "one"))
            .await
            .unwrap()
            .action["state"],
        "denied"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn control_revoked_library_source_cannot_authorize_a_business_action() {
    use crate::identity::library::model::{SavePage, ShareItem};
    let (db, owner, w, connection, job) = setup("ask").await;
    let item = uuid::Uuid::new_v4().to_string();
    db.library_save(
        &owner,
        &w,
        &SavePage {
            id: item.clone(),
            request: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            title: "Approved instructions".into(),
            document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Approved order instructions"}]}]}),
        },
    )
    .await
    .unwrap();
    let mut grant = ShareItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        visibility: "private".into(),
        people: vec![],
        agents: vec![job.agent.clone()],
    };
    db.library_share(&owner, &w, &item, &grant).await.unwrap();
    let current = db.library_get(&owner, &w, &item).await.unwrap();
    execute(
        &db.db,
        "UPDATE relay_chats SET visibility='private' WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), job.chat.clone().into()],
    )
    .await
    .unwrap();
    execute(&db.db, "INSERT INTO relay_library_sources(workspace,reply,item,revision,access_revision) VALUES($1,$2,$3,$4,$5)", vec![w.clone().into(),job.reply.clone().into(),item.into(),current["revision"].as_i64().unwrap().into(),current["access_revision"].as_i64().unwrap().into()]).await.unwrap();
    let proposed = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    grant.request = uuid::Uuid::new_v4().to_string();
    grant.revision = 2;
    grant.agents.clear();
    db.library_share(&owner, &w, current["id"].as_str().unwrap(), &grant)
        .await
        .unwrap();
    assert!(db.control_operations(&job).await.is_err());
    assert!(
        db.control_propose(&job, &request(&connection, "two"))
            .await
            .is_err()
    );
    assert!(
        db.control_decide(
            &owner,
            &w,
            &job.agent,
            proposed.action["id"].as_str().unwrap(),
            &decision("approve")
        )
        .await
        .is_err()
    );
}
