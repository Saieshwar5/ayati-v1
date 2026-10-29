use super::{tests::*, *};
use crate::identity::{
    chat::execution::prepare,
    communications::model::{Binding, Endpoint, Message, Outcome, Transport},
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communications_migration_preserves_inflight_v31_chat_and_starts_receipts_without_backfill()
{
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!(
        "relay_communication_upgrade_{}",
        uuid::Uuid::new_v4().simple()
    );
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(scoped.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(31)).await.unwrap();
    let owner = user(&db, "upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_send(&owner, &w, &c, &message("Existing request", Some(&a)))
        .await
        .unwrap();
    let old = db.claim_job().await.unwrap().unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    db.chat_finish(&old, "Existing reply", "complete")
        .await
        .unwrap();
    assert!(
        db.communication_status(&owner, &w).await.unwrap()["inbox"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    db.chat_send(&owner, &w, &c, &message("New request", Some(&a)))
        .await
        .unwrap();
    let new = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&new, "New reply", "complete").await.unwrap();
    let status = db.communication_status(&owner, &w).await.unwrap();
    assert_eq!(status["inbox"].as_array().unwrap().len(), 1);
    assert_eq!(status["outbox"][0]["state"], "published");
    assert!(
        db.chat_messages(&owner, &w, &c)
            .await
            .unwrap()
            .to_string()
            .contains("Existing reply")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communications_dispatch_rechecks_context_and_database_fences_exact_envelopes() {
    let db = fixture().await;
    let owner = user(&db, "source-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let endpoint = Endpoint {
        id: uuid::Uuid::new_v4().to_string(),
        agent: a.clone(),
        transport: Transport::Email,
        address: "agent@simulation.invalid".into(),
    };
    db.communication_endpoint(&owner, &w, &endpoint)
        .await
        .unwrap();
    db.communication_binding(
        &owner,
        &w,
        &endpoint.id,
        &Binding {
            sender: "user@simulation.invalid".into(),
            user: owner.id.clone(),
        },
    )
    .await
    .unwrap();
    db.simulate_incoming(
        &owner,
        &w,
        &endpoint.id,
        &Message {
            event: "message".into(),
            sender: "user@simulation.invalid".into(),
            recipient: endpoint.address.clone(),
            conversation: "thread".into(),
            text: "Help".into(),
            attachments: vec![],
            automated: false,
        },
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    prepare(&db, &job, 180).await.unwrap();
    db.chat_finish(&job, "Reply from older context", "complete")
        .await
        .unwrap();
    assert!(execute(&db.db,"UPDATE relay_communication_outbox SET envelope=jsonb_set(envelope,'{recipient}','\"wrong@simulation.invalid\"') WHERE workspace=$1",vec![w.clone().into()]).await.is_err());
    assert!(execute(&db.db,"UPDATE relay_communication_endpoints SET address='wrong@simulation.invalid' WHERE workspace=$1",vec![w.clone().into()]).await.is_err());
    assert!(
        execute(
            &db.db,
            "DELETE FROM relay_communication_inbox WHERE workspace=$1",
            vec![w.clone().into()]
        )
        .await
        .is_err()
    );
    // Change the exact evidence after completion but before dispatch.
    let request: String = query(
        &db.db,
        "SELECT request_message FROM relay_chat_jobs WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "request_message")
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET content='Corrected source' WHERE id=$1",
        vec![request.into()],
    )
    .await
    .unwrap();
    let result = db
        .simulate_dispatch(&owner, &w, &job.reply, Outcome::Delivered)
        .await
        .unwrap();
    assert_eq!(result["state"], "cancelled");
    assert_eq!(result["attempts"], 0);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communications_revoked_agent_grant_blocks_inflight_reply_and_new_intake() {
    let db = fixture().await;
    let owner = user(&db, "revoke-owner").await;
    let member = user(&db, "revoke-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "User").await;
    let (_, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &member.id, &a, true)
        .await
        .unwrap();
    let endpoint = Endpoint {
        id: uuid::Uuid::new_v4().to_string(),
        agent: a.clone(),
        transport: Transport::Whatsapp,
        address: "sim-agent".into(),
    };
    db.communication_endpoint(&owner, &w, &endpoint)
        .await
        .unwrap();
    db.communication_binding(
        &owner,
        &w,
        &endpoint.id,
        &Binding {
            sender: "sim-person".into(),
            user: member.id.clone(),
        },
    )
    .await
    .unwrap();
    let mut input = Message {
        event: "message".into(),
        sender: "sim-person".into(),
        recipient: endpoint.address.clone(),
        conversation: "wa-conversation".into(),
        text: "Help".into(),
        attachments: vec![],
        automated: false,
    };
    db.simulate_incoming(&owner, &w, &endpoint.id, &input)
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_grant(&owner, &w, &member.id, &a, false)
        .await
        .unwrap();
    assert!(!db.chat_job_allowed(&job).await);
    db.chat_finish(&job, "MUST_NOT_SEND", "complete")
        .await
        .unwrap();
    assert_eq!(
        db.communication_status(&owner, &w).await.unwrap()["outbox"],
        json!([])
    );
    input.event = "after-revoke".into();
    assert_eq!(
        db.simulate_incoming(&owner, &w, &endpoint.id, &input)
            .await
            .unwrap()["status"],
        "held"
    );
    assert!(db.claim_job().await.unwrap().is_none());
}
