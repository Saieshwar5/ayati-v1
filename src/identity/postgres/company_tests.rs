use super::query;
use super::tests::{chat, fixture, join, settings_input, user, workspace};
use crate::identity::{
    chat::model::{NewChat, NewMessage},
    collaboration::{ChannelSettings, Feedback, Object},
};
use serde_json::{Value, json};
fn message(content: &str) -> NewMessage {
    NewMessage {
        reply_to: None,
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
    db.save_object(&owner, &w, &c, &obj, false).await.unwrap();
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
        &settings_input("public", true, Some("Company announcements")),
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
    assert!(error.is::<crate::identity::chat::model::QueueFull>());
    let counts = db.company_counts(&owner, &w).await.unwrap();
    assert_eq!(counts["pending"], 64);
    assert_eq!(counts["messages"], json!(225 + 128));
}

/// The conversation as `person`'s chat list shows it, if they can see it.
async fn listed(db: &super::PgStore, person: &super::User, w: &str, chat: &str) -> Option<Value> {
    let list = db.chats(person, w).await.unwrap();
    list["chats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == chat)
        .cloned()
}
async fn job_state(db: &super::PgStore, reply: &str) -> String {
    query(
        &db.db,
        "SELECT state FROM relay_chat_jobs WHERE reply=$1",
        vec![reply.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "state")
    .unwrap()
}
async fn audited(db: &super::PgStore, w: &str, action: &str) -> i64 {
    query(
        &db.db,
        "SELECT count(*) AS n FROM relay_audit WHERE workspace=$1 AND action=$2",
        vec![w.into(), action.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn channel_managers_edit_name_description_and_posting_but_only_admins_change_visibility() {
    let db = fixture().await;
    let admin = user(&db, "settings-admin").await;
    let creator = user(&db, "settings-creator").await;
    let member = user(&db, "settings-member").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &creator, "Member").await;
    join(&db, &admin, &w, &member, "Member").await;
    let c = chat(
        &db,
        &creator,
        &w,
        vec![member.id.clone(), admin.id.clone()],
        vec![],
    )
    .await;
    let edit = |visibility: &str, description: Option<&str>| ChannelSettings {
        name: Some("  dispatch  ".into()),
        description: description.map(str::to_owned),
        visibility: visibility.into(),
        announcements: true,
    };
    // The creator, a Member, changes the name, description and who can post.
    db.channel_settings(
        &creator,
        &w,
        &c,
        &edit("private", Some(" Slots and invoices ")),
    )
    .await
    .unwrap();
    let saved = listed(&db, &member, &w, &c).await.unwrap();
    assert_eq!(
        (
            saved["name"].as_str(),
            saved["description"].as_str(),
            saved["announcements"].as_bool()
        ),
        (Some("dispatch"), Some("Slots and invoices"), Some(true))
    );
    // Leaving the description out keeps it.
    db.channel_settings(&creator, &w, &c, &settings_input("private", false, None))
        .await
        .unwrap();
    assert_eq!(
        listed(&db, &member, &w, &c).await.unwrap()["description"],
        "Slots and invoices"
    );
    // Who can find it changes who can read its history: admins only.
    let refused = db
        .channel_settings(&creator, &w, &c, &edit("public", None))
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("Only workspace admins"),
        "{refused}"
    );
    assert!(
        db.channel_settings(&member, &w, &c, &edit("private", Some("Mine now")))
            .await
            .is_err()
    );
    let outsider = user(&db, "settings-outsider").await;
    join(&db, &admin, &w, &outsider, "Member").await;
    assert!(
        db.channel_settings(&outsider, &w, &c, &edit("private", None))
            .await
            .is_err()
    );
    assert_eq!(
        listed(&db, &member, &w, &c).await.unwrap()["visibility"],
        "private"
    );
    db.channel_settings(&admin, &w, &c, &edit("public", Some("Open to all")))
        .await
        .unwrap();
    assert_eq!(
        listed(&db, &outsider, &w, &c).await.unwrap()["description"],
        "Open to all"
    );
    // Lengths follow the rules for creating a channel.
    assert!(
        db.channel_settings(&admin, &w, &c, &edit("public", Some(&"x".repeat(301))))
            .await
            .is_err()
    );
    assert!(
        db.channel_settings(&admin, &w, &c, &settings_input("public", false, Some(" ")))
            .await
            .is_err()
    );
    assert!(
        db.channel_settings(&admin, &w, &c, &settings_input("hidden", false, None))
            .await
            .is_err()
    );
    let dm = direct(&db, &member, &w, &creator).await;
    assert!(
        db.channel_settings(&member, &w, &dm, &settings_input("private", false, None))
            .await
            .is_err()
    );
    assert_eq!(audited(&db, &w, "channel_settings_updated").await, 3);
}

async fn direct(db: &super::PgStore, from: &super::User, w: &str, to: &super::User) -> String {
    let input = NewChat {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Direct".into(),
        users: vec![to.id.clone()],
        agents: vec![],
        kind: "direct".into(),
        description: String::new(),
    };
    db.chat_create(from, w, &input).await.unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn leaving_a_channel_ends_access_unread_and_waiting_agent_work() {
    let (db, owner, w, connection, initial) = super::control_tests::setup("ask").await;
    db.chat_finish(&initial, "No operation needed", "complete")
        .await
        .unwrap();
    let member = user(&db, "leaving-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    db.channel_members(
        &owner,
        &w,
        &initial.chat,
        &[owner.id.clone(), member.id.clone()],
    )
    .await
    .unwrap();
    let mut ask = message("Update an order");
    ask.agent = Some(initial.agent.clone());
    db.chat_send(&member, &w, &initial.chat, &ask)
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.control_propose(&job, &super::control_tests::request(&connection, "left"))
        .await
        .unwrap();
    db.chat_finish(&job, "Waiting for approval", "complete")
        .await
        .unwrap();
    assert_eq!(job_state(&db, &job.reply).await, "waiting");
    db.chat_send(&owner, &w, &initial.chat, &message("Anyone free?"))
        .await
        .unwrap();
    let unread = db.unread(&member, &w).await.unwrap();
    assert!(
        unread["unread"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["chat"] == initial.chat)
    );

    db.channel_leave(&member, &w, &initial.chat).await.unwrap();
    assert!(listed(&db, &member, &w, &initial.chat).await.is_none());
    assert!(
        db.unread(&member, &w).await.unwrap()["unread"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(db.chat_messages(&member, &w, &initial.chat).await.is_err());
    assert!(
        db.chat_send(&member, &w, &initial.chat, &message("Still here?"))
            .await
            .is_err()
    );
    assert_eq!(
        job_state(&db, &job.reply).await,
        "finished",
        "waiting work stops"
    );
    assert!(!db.chat_job_allowed(&job).await);
    assert_eq!(
        listed(&db, &owner, &w, &initial.chat).await.unwrap()["users"],
        json!([owner.id])
    );
    // Once gone, the private channel is not theirs to leave again.
    assert!(db.channel_leave(&member, &w, &initial.chat).await.is_err());
    assert_eq!(audited(&db, &w, "channel_left").await, 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_last_person_who_can_manage_a_private_channel_cannot_leave_it() {
    let db = fixture().await;
    let admin = user(&db, "last-admin").await;
    let creator = user(&db, "last-creator").await;
    let member = user(&db, "last-member").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &creator, "Member").await;
    join(&db, &admin, &w, &member, "Member").await;
    let private = chat(&db, &creator, &w, vec![member.id.clone()], vec![]).await;
    let refused = db.channel_leave(&creator, &w, &private).await.unwrap_err();
    assert!(
        refused.to_string().contains("Add a workspace admin"),
        "{refused}"
    );
    assert!(
        listed(&db, &creator, &w, &private).await.is_some(),
        "nothing changed"
    );
    // People who do not manage it may always leave.
    db.channel_leave(&member, &w, &private).await.unwrap();
    // With an admin in it, the creator may go; the admin is then the last manager.
    db.channel_members(
        &creator,
        &w,
        &private,
        &[creator.id.clone(), admin.id.clone()],
    )
    .await
    .unwrap();
    db.channel_leave(&creator, &w, &private).await.unwrap();
    assert!(db.channel_leave(&admin, &w, &private).await.is_err());
    // Every admin manages a public channel, so anyone may leave it. It stays
    // listed and readable like every public channel; leaving again changes nothing.
    let public = chat(&db, &admin, &w, vec![member.id.clone()], vec![]).await;
    db.channel_settings(&admin, &w, &public, &settings_input("public", false, None))
        .await
        .unwrap();
    db.channel_leave(&admin, &w, &public).await.unwrap();
    db.channel_leave(&admin, &w, &public).await.unwrap();
    let after = listed(&db, &admin, &w, &public).await.unwrap();
    assert_eq!(
        (after["users"].clone(), after["can_archive"].clone()),
        (json!([member.id]), json!(true))
    );
    let dm = direct(&db, &member, &w, &creator).await;
    assert!(db.channel_leave(&member, &w, &dm).await.is_err());
    assert_eq!(audited(&db, &w, "channel_left").await, 3);
}
