//! Notification levels are personal and never grant access; unread counts
//! separately count what is for you.
use super::tests::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn levels_are_personal_and_need_access_to_the_conversation() {
    let db = fixture().await;
    let owner = user(&db, "notify-owner").await;
    let w = workspace(&db, &owner).await;
    let member = user(&db, "notify-member").await;
    join(&db, &owner, &w, &member, "User").await;
    let private = chat(&db, &owner, &w, vec![], vec![]).await;
    let shared = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    assert!(
        db.set_chat_notify(&member, &w, &private, "all")
            .await
            .is_err(),
        "no access, no setting"
    );
    assert!(
        db.set_chat_notify(&member, &w, &shared, "loud")
            .await
            .is_err()
    );
    db.set_chat_notify(&member, &w, &shared, "none")
        .await
        .unwrap();
    db.set_chat_notify(&member, &w, &shared, "all")
        .await
        .unwrap();
    assert_eq!(
        db.chat_notify_levels(&member, &w).await.unwrap()["levels"],
        serde_json::json!({ shared.clone(): "all" })
    );
    assert_eq!(
        db.chat_notify_levels(&owner, &w).await.unwrap()["levels"],
        serde_json::json!({})
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn unread_counts_separate_mentions_of_you() {
    let db = fixture().await;
    let owner = user(&db, "mention-owner").await;
    let w = workspace(&db, &owner).await;
    let member = user(&db, "mention-member").await;
    join(&db, &owner, &w, &member, "User").await;
    let c = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    db.chat_send(&owner, &w, &c, &message("General update", None))
        .await
        .unwrap();
    let mention = format!("[@Member](mention:user:{}) please check", member.id);
    db.chat_send(&owner, &w, &c, &message(&mention, None))
        .await
        .unwrap();
    // A mention of someone else is not for this member.
    db.chat_send(
        &owner,
        &w,
        &c,
        &message(&format!("[@Owner](mention:user:{}) note", owner.id), None),
    )
    .await
    .unwrap();
    let unread = db.unread(&member, &w).await.unwrap();
    let entry = unread["unread"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["chat"] == c)
        .unwrap()
        .clone();
    assert_eq!(
        (entry["count"].as_i64(), entry["mentions"].as_i64()),
        (Some(3), Some(1))
    );
    // Live updates tell the browser which new messages are for this person.
    let cursor = db.sync_page(&member, &w, None).await.unwrap()["cursor"]
        .as_i64()
        .unwrap();
    let again = message(&mention, None);
    db.chat_send(&owner, &w, &c, &again).await.unwrap();
    let page = db.sync_page(&member, &w, Some(cursor)).await.unwrap();
    let live = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["message"]["id"] == again.id)
        .unwrap()
        .clone();
    assert_eq!(live["message"]["for_me"], true);
    let owners = db.sync_page(&owner, &w, Some(cursor)).await.unwrap();
    let theirs = owners["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["message"]["id"] == again.id)
        .unwrap()
        .clone();
    assert_eq!(theirs["message"]["for_me"], false);
}
