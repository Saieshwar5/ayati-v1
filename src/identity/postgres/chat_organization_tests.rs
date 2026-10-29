use super::{control_tests::setup, tests::*, *};
use crate::identity::chat::organization_model::{Lifecycle, Pin, PinKind, SavePins};
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
    join(&db, &owner, &w, &member, "User").await;
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
    join(&db, &owner, &w, &creator, "User").await;
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
