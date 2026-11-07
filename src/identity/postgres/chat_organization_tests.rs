use super::{
    control_tests::{request, setup},
    tests::*,
    *,
};
use crate::identity::chat::{
    model::NewChat,
    organization_model::{Lifecycle, Pin, PinKind, SavePins},
};
fn lifecycle(archived: bool, revision: i64) -> Lifecycle {
    Lifecycle {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        archived,
    }
}
fn pins(revision: i64, ids: &[&str]) -> SavePins {
    SavePins {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        pins: ids
            .iter()
            .map(|id| Pin {
                kind: PinKind::Chat,
                id: (*id).into(),
            })
            .collect(),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pins_are_personal_ordered_scoped_and_revision_checked_without_creating_work() {
    let (db, owner, w, _, job) = setup("ask").await;
    let member = user(&db, "pin-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    db.channel_members(
        &owner,
        &w,
        &job.chat,
        &[owner.id.clone(), member.id.clone()],
    )
    .await
    .unwrap();
    let other = chat(&db, &owner, &w, vec![], vec![]).await;
    let before = db.company_counts(&owner, &w).await.unwrap();
    let input = pins(0, &[&other, &job.chat]);
    let (a, b) = tokio::join!(
        db.save_chat_pins(&owner, &w, &input),
        db.save_chat_pins(&owner, &w, &input)
    );
    assert_eq!(a.unwrap(), b.unwrap());
    let saved = db.chat_pins(&owner, &w).await.unwrap();
    assert_eq!(saved["revision"], 1);
    assert_eq!(saved["pins"][0]["id"], other);
    assert_eq!(
        db.chat_pins(&member, &w).await.unwrap()["pins"],
        serde_json::json!([])
    );
    assert!(
        db.save_chat_pins(&member, &w, &pins(0, &[&other]))
            .await
            .is_err()
    );
    assert!(
        db.save_chat_pins(&owner, &workspace(&db, &owner).await, &pins(0, &[&other]))
            .await
            .is_err()
    );
    assert!(
        db.save_chat_pins(&owner, &w, &pins(0, &[&job.chat]))
            .await
            .is_err()
    );
    let mut changed = pins(0, &[&job.chat]);
    changed.request = input.request.clone();
    assert!(db.save_chat_pins(&owner, &w, &changed).await.is_err());
    assert!(
        db.save_chat_pins(&owner, &w, &pins(1, &[&other, &other]))
            .await
            .is_err()
    );
    assert_eq!(before, db.company_counts(&owner, &w).await.unwrap());
    db.save_chat_pins(&member, &w, &pins(0, &[&job.chat]))
        .await
        .unwrap();
    db.channel_members(&owner, &w, &job.chat, std::slice::from_ref(&owner.id))
        .await
        .unwrap();
    assert_eq!(
        db.chat_pins(&member, &w).await.unwrap()["pins"],
        serde_json::json!([])
    );
    let revoked = db.organized_chats(&member, &w).await.unwrap();
    assert_eq!(revoked["preferences"]["pins"], serde_json::json!([]));
    assert!(
        !revoked["chats"]
            .as_array()
            .unwrap()
            .iter()
            .any(|chat| chat["id"] == job.chat)
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn archive_closes_new_intake_but_keeps_exact_acknowledgments_results_and_history() {
    let (db, owner, w, _, job) = setup("ask").await;
    let accepted = message("Previously accepted ordinary message", None);
    db.chat_send(&owner, &w, &job.chat, &accepted)
        .await
        .unwrap();
    let archive = lifecycle(true, 0);
    assert_eq!(
        db.channel_lifecycle(&owner, &w, &job.chat, None)
            .await
            .unwrap()["active_requests"],
        1
    );
    let saved = db
        .channel_lifecycle(&owner, &w, &job.chat, Some(&archive))
        .await
        .unwrap();
    assert_eq!(saved["archived"], true);
    assert_eq!(
        db.channel_lifecycle(&owner, &w, &job.chat, Some(&archive))
            .await
            .unwrap(),
        saved
    );
    assert!(
        db.chat_send(&owner, &w, &job.chat, &accepted)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.chat_send(&owner, &w, &job.chat, &message("New ordinary", None))
            .await
            .is_err()
    );
    assert!(
        db.chat_send(
            &owner,
            &w,
            &job.chat,
            &message("New work", Some(&job.agent))
        )
        .await
        .is_err()
    );
    let mut pool = db.clone();
    pool.pooled = true;
    assert!(
        pool.chat_background(
            &owner,
            &w,
            &job.chat,
            &message("New background work", Some(&job.agent))
        )
        .await
        .is_err()
    );
    assert!(
        db.check_chat_accepting(&owner, &w, &job.chat)
            .await
            .is_err()
    );
    assert!(execute(&db.db,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) VALUES($1,$2,$3,$4,'user','Must not bypass archive','sent',0)",vec![uuid::Uuid::new_v4().to_string().into(),w.clone().into(),job.chat.clone().into(),owner.id.clone().into()]).await.is_err());
    assert!(
        db.finish_claimed_job(&job, "Previously accepted work finished", "complete")
            .await
            .unwrap()
    );
    assert!(
        db.chat_messages(&owner, &w, &job.chat)
            .await
            .unwrap()
            .to_string()
            .contains("Previously accepted work finished")
    );
    assert!(
        db.channel_lifecycle(&owner, &w, &job.chat, Some(&lifecycle(false, 0)))
            .await
            .is_err()
    );
    db.channel_lifecycle(&owner, &w, &job.chat, Some(&lifecycle(false, 1)))
        .await
        .unwrap();
    assert!(
        db.channel_lifecycle(&owner, &w, &job.chat, Some(&archive))
            .await
            .is_err()
    );
    db.chat_send(
        &owner,
        &w,
        &job.chat,
        &message("New ordinary message after restore", None),
    )
    .await
    .unwrap();
    assert!(db.claim_job().await.unwrap().is_none());
    assert_eq!(
        db.channel_lifecycle(&owner, &w, &job.chat, None)
            .await
            .unwrap()["revision"],
        2
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn archive_requires_current_source_and_creator_or_owner_authority() {
    let (db, owner, w, _, job) = setup("ask").await;
    let creator = user(&db, "channel-creator").await;
    join(&db, &owner, &w, &creator, "Member").await;
    db.channel_members(
        &owner,
        &w,
        &job.chat,
        &[owner.id.clone(), creator.id.clone()],
    )
    .await
    .unwrap();
    assert!(
        !db.channel_lifecycle(&creator, &w, &job.chat, None)
            .await
            .unwrap()["manageable"]
            .as_bool()
            .unwrap()
    );
    assert!(
        db.channel_lifecycle(&creator, &w, &job.chat, Some(&lifecycle(true, 0)))
            .await
            .is_err()
    );
    let private = chat(&db, &creator, &w, vec![], vec![]).await;
    assert!(
        db.channel_lifecycle(&owner, &w, &private, None)
            .await
            .is_err()
    );
    assert!(
        db.channel_lifecycle(&owner, &w, &private, Some(&lifecycle(true, 0)))
            .await
            .is_err()
    );
    assert_eq!(
        db.channel_lifecycle(&creator, &w, &private, Some(&lifecycle(true, 0)))
            .await
            .unwrap()["archived"],
        true
    );
    let stranger = user(&db, "archive-stranger").await;
    assert!(
        db.channel_lifecycle(&stranger, &w, &private, None)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn archiving_keeps_saved_work_and_questions_current() {
    let (db, owner, w, _, job) = setup("ask").await;
    let epochs = || async {
        query(&db.db, "SELECT COALESCE(sum(revision),0)::bigint AS n FROM relay_context_epochs WHERE workspace=$1", vec![w.clone().into()])
            .await.unwrap().map(|r| r.try_get::<i64>("", "n").unwrap()).unwrap_or(0)
    };
    let before = epochs().await;
    db.channel_lifecycle(&owner, &w, &job.chat, Some(&lifecycle(true, 0)))
        .await
        .unwrap();
    db.channel_lifecycle(&owner, &w, &job.chat, Some(&lifecycle(false, 1)))
        .await
        .unwrap();
    assert_eq!(
        epochs().await,
        before,
        "archive and restore change intake, not who can read"
    );
}

fn listed(list: &Value, id: &str) -> Value {
    let chats = list["chats"].as_array().unwrap();
    chats.iter().find(|c| c["id"] == id).cloned().unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn channels_list_their_creator_creation_time_and_managers() {
    let db = fixture().await;
    let admin = user(&db, "list-admin").await;
    let creator = user(&db, "list-creator").await;
    let member = user(&db, "list-member").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &creator, "Member").await;
    join(&db, &admin, &w, &member, "Member").await;
    let before = now();
    let people = vec![member.id.clone(), admin.id.clone()];
    let c = chat(&db, &creator, &w, people, vec![]).await;
    let channel = listed(&db.organized_chats(&member, &w).await.unwrap(), &c);
    assert_eq!(channel["creator"], creator.id);
    let at = channel["created_at"].as_i64().unwrap();
    assert!((before..=now()).contains(&at), "{at}");
    let mut managers: Vec<_> = channel["managers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    managers.sort();
    let mut expected = vec![admin.id.as_str(), creator.id.as_str()];
    expected.sort();
    assert_eq!(
        managers, expected,
        "the creator and admins, not other members"
    );
    assert_eq!(channel["can_archive"], false, "the member manages nothing");
    // Direct messages show no creator or managers, and their creation is not
    // recorded: admins never see who messages whom.
    let input = NewChat {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Direct".into(),
        users: vec![creator.id.clone()],
        agents: vec![],
        kind: "direct".into(),
        description: String::new(),
    };
    let dm = db.chat_create(&member, &w, &input).await.unwrap();
    let direct = listed(&db.organized_chats(&member, &w).await.unwrap(), &dm);
    assert!(direct.get("creator").is_none() && direct.get("managers").is_none());
    let recorded: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_audit WHERE workspace=$1 AND action='channel_created'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(recorded, 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_archive_review_lists_bounded_unfinished_work_only_for_people_who_may_archive() {
    let (db, owner, w, connection, job) = setup("ask").await;
    db.control_propose(&job, &request(&connection, "review"))
        .await
        .unwrap();
    db.chat_finish(&job, "Waiting for approval", "complete")
        .await
        .unwrap();
    let member = user(&db, "review-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let people = [owner.id.clone(), member.id.clone()];
    db.channel_members(&owner, &w, &job.chat, &people)
        .await
        .unwrap();
    for i in 0..22 {
        let ask = message(&format!("Request {i}"), Some(&job.agent));
        db.chat_send(&owner, &w, &job.chat, &ask).await.unwrap();
    }
    let review = db
        .channel_lifecycle(&owner, &w, &job.chat, None)
        .await
        .unwrap();
    assert_eq!(review["active_requests"], 23);
    let items = review["unfinished"].as_array().unwrap();
    assert_eq!((items.len(), &review["more"]), (20, &serde_json::json!(3)));
    assert_eq!(items[0]["request"], job.reply);
    assert_eq!(items[0]["state"], "approval");
    assert_eq!(items[0]["summary"], "Update the sample order");
    assert_eq!(items[0]["agent_name"], "Pool agent");
    assert!(items[0]["started_ms"].as_i64().unwrap() > 0);
    assert_eq!(items[1]["state"], "queued");
    assert_eq!(items[1]["summary"], "Request 0");
    // Someone who may not archive sees how much is unfinished, not what.
    let plain = db
        .channel_lifecycle(&member, &w, &job.chat, None)
        .await
        .unwrap();
    assert_eq!(plain["active_requests"], 23);
    assert!(plain.get("unfinished").is_none() && plain.get("more").is_none());
}
