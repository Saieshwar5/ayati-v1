use super::tests::{chat, fixture, join, user, workspace};
use crate::identity::{
    chat::store::NewMessage,
    collaboration::{Feedback, Object},
};
use serde_json::json;
fn message(content: &str) -> NewMessage {
    NewMessage {
        id: uuid::Uuid::new_v4().to_string(),
        content: content.into(),
        agent: None,
        thread: None,
        attachments: vec![],
    }
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn company_media_feedback_and_membership_boundaries() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let member = user(&db, "member").await;
    let outsider = user(&db, "outsider").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    join(&db, &owner, &w, &outsider, "Member").await;
    let c = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    let obj = Object {
        id: uuid::Uuid::new_v4().to_string(),
        name: "invoice.txt".into(),
        mime: "application/octet-stream".into(),
        size: 10,
        checksum: "abc".into(),
    };
    db.save_object(&owner, &w, &c, &obj).await.unwrap();
    assert!(db.object(&member, &w, &obj.id).await.is_err());
    let mut m = message("Invoice attached");
    m.attachments.push(obj.id.clone());
    db.chat_send(&owner, &w, &c, &m).await.unwrap();
    assert_eq!(db.object(&member, &w, &obj.id).await.unwrap().size, 10);
    assert!(db.object(&outsider, &w, &obj.id).await.is_err());
    let c2 = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    m.id = uuid::Uuid::new_v4().to_string();
    assert!(db.chat_send(&owner, &w, &c2, &m).await.is_err());
    let feedback = Feedback {
        id: uuid::Uuid::new_v4().to_string(),
        chat: c.clone(),
        title: "Invoice preview".into(),
        expected: "Document readable".into(),
        actual: "Download only".into(),
        category: "missing-feature".into(),
        severity: "low".into(),
        evidence: obj.id.clone(),
    };
    db.add_feedback(&member, &w, &feedback).await.unwrap();
    db.add_feedback(&member, &w, &feedback).await.unwrap();
    assert_eq!(db.feedback(&member, &w).await.unwrap().len(), 1);
    assert!(db.feedback(&outsider, &w).await.unwrap().is_empty());
    assert!(
        db.feedback_status(&member, &w, &feedback.id, "verified", 0)
            .await
            .is_err()
    );
    db.feedback_status(&owner, &w, &feedback.id, "investigating", 0)
        .await
        .unwrap();
    assert!(
        db.feedback_status(&owner, &w, &feedback.id, "verified", 0)
            .await
            .is_err()
    );
    db.channel_members(&owner, &w, &c, std::slice::from_ref(&owner.id))
        .await
        .unwrap();
    assert!(db.object(&member, &w, &obj.id).await.is_err());
    assert!(db.feedback(&member, &w).await.unwrap().is_empty());
    db.channel_settings(
        &owner,
        &w,
        &c,
        "public",
        true,
        Some("Company announcements"),
    )
    .await
    .unwrap();
    assert!(db.object(&outsider, &w, &obj.id).await.is_ok());
    assert!(
        db.chat_send(&outsider, &w, &c, &message("Unauthorized announcement"))
            .await
            .is_err()
    );
    db.chat_send(&owner, &w, &c, &message("Owner announcement"))
        .await
        .unwrap();
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agent_work_requires_grant_approval_and_current_fenced_claim() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let w = workspace(&db, &owner).await;
    let agent = uuid::Uuid::new_v4().to_string();
    db.save_shared_agent(&owner, &w, &agent, "Sales", "Quotes", "scripted")
        .await
        .unwrap();
    let c = chat(&db, &owner, &w, vec![], vec![agent.clone()]).await;
    let mut m = message("[quote]");
    m.agent = Some(agent.clone());
    db.chat_send(&owner, &w, &c, &m).await.unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    assert!(db.scripted_work(&job).await.is_err());
    db.work_grant(&owner, &w, &c, &agent, true).await.unwrap();
    let result = db.scripted_work(&job).await.unwrap();
    assert_eq!(db.scripted_work(&job).await.unwrap(), result);
    let mut stale = job.clone();
    stale.claim = Some("stale".into());
    assert!(db.scripted_work(&stale).await.is_err());
    assert_eq!(db.company_work(&owner, &w).await.unwrap().len(), 1);
    db.chat_finish(&job, &result, "complete").await.unwrap();
    let mut complete = message(&format!("[complete:{}]", job.reply));
    complete.agent = Some(agent.clone());
    db.chat_send(&owner, &w, &c, &complete).await.unwrap();
    let completion = db.claim_job().await.unwrap().unwrap();
    assert!(db.scripted_work(&completion).await.is_err());
    db.work_update(&owner, &w, &job.reply, "approved", &owner.id, 0)
        .await
        .unwrap();
    db.scripted_work(&completion).await.unwrap();
    assert_eq!(
        db.company_work(&owner, &w).await.unwrap()[0]["status"],
        "completed"
    );
    db.work_grant(&owner, &w, &c, &agent, false).await.unwrap();
    assert!(db.scripted_work(&completion).await.is_err());
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn queue_bounds_and_history_pages_retain_all_records() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec!["agent".into()]).await;
    for i in 0..225 {
        db.chat_send(&owner, &w, &c, &message(&format!("record {i}")))
            .await
            .unwrap();
    }
    let first = db.chat_page(&owner, &w, &c, None).await.unwrap();
    assert_eq!(first["messages"].as_array().unwrap().len(), 200);
    let second = db
        .chat_page(&owner, &w, &c, first["next_before"].as_i64())
        .await
        .unwrap();
    assert_eq!(second["messages"].as_array().unwrap().len(), 25);
    assert!(second["next_before"].is_null());
    for _ in 0..64 {
        let mut m = message("queued");
        m.agent = Some("agent".into());
        db.chat_send(&owner, &w, &c, &m).await.unwrap();
    }
    let mut m = message("overflow");
    m.agent = Some("agent".into());
    let error = db.chat_send(&owner, &w, &c, &m).await.unwrap_err();
    assert!(error.is::<crate::identity::chat::store::QueueFull>());
    let counts = db.company_counts(&owner, &w).await.unwrap();
    assert_eq!(counts["pending"], 64);
    assert_eq!(counts["messages"], json!(225 + 128));
}
