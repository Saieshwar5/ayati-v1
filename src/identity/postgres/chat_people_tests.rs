//! Everyone sees every profile in their workspace; only the person edits theirs.
//! Channel managers choose which workspace agents work in a channel.
use super::{tests::*, *};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nphoto";

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn every_member_sees_everyone_with_their_profile() {
    let db = fixture().await;
    let owner = user(&db, "profile-owner").await;
    let w = workspace(&db, &owner).await;
    let member = user(&db, "profile-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let saved = db
        .save_member_profile(
            &member,
            &w,
            " Meera Iyer ",
            "Design lead",
            "Mood boards and fabrics.",
        )
        .await
        .unwrap();
    assert_eq!(saved["name"], "Meera Iyer");
    assert_eq!(saved["title"], "Design lead");
    assert!(saved["photo"].is_null());
    // The owner's directory lists both people, with the member's profile.
    let directory = db.directory(&owner, &w).await.unwrap();
    let users = directory["users"].as_array().unwrap();
    assert_eq!(users.len(), 2);
    let meera = users.iter().find(|u| u["id"] == member.id).unwrap();
    assert_eq!(meera["name"], "Meera Iyer");
    assert_eq!(meera["about"], "Mood boards and fabrics.");
    let own = users.iter().find(|u| u["id"] == owner.id).unwrap();
    assert_eq!(own["title"], "", "a person without a profile still appears");
    // The ordinary member sees the owner too: nobody is hidden.
    assert_eq!(
        db.directory(&member, &w).await.unwrap()["users"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn profile_text_is_bounded_and_rejected_whole() {
    let db = fixture().await;
    let owner = user(&db, "profile-bounds").await;
    let w = workspace(&db, &owner).await;
    db.save_member_profile(&owner, &w, "Priya", "Founder", "Runs the studio.")
        .await
        .unwrap();
    for (name, title, about) in [
        ("", "Founder", ""),
        ("Priya", &"x".repeat(81) as &str, ""),
        ("Priya", "Founder", &"y".repeat(201) as &str),
        ("Priya", "Bad\0title", ""),
    ] {
        assert!(
            db.save_member_profile(&owner, &w, name, title, about)
                .await
                .is_err()
        );
    }
    // A failed save leaves the previous profile untouched.
    let me = db.directory(&owner, &w).await.unwrap()["users"][0].clone();
    assert_eq!(
        (me["name"].as_str(), me["title"].as_str()),
        (Some("Priya"), Some("Founder"))
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn photos_are_member_only_replaced_by_new_ids_and_removable() {
    let db = fixture().await;
    let owner = user(&db, "photo-owner").await;
    let w = workspace(&db, &owner).await;
    let member = user(&db, "photo-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let first = db
        .save_member_photo(&member, &w, "image/png", PNG.to_vec())
        .await
        .unwrap();
    let first_id = first["photo"].as_str().unwrap().to_owned();
    let (mime, bytes) = db
        .member_photo(&owner, &w, &member.id, &first_id)
        .await
        .unwrap();
    assert_eq!((mime.as_str(), bytes.as_slice()), ("image/png", PNG));
    // Someone outside the workspace cannot read it.
    let outsider = user(&db, "photo-outsider").await;
    assert!(
        db.member_photo(&outsider, &w, &member.id, &first_id)
            .await
            .is_err()
    );
    let second = db
        .save_member_photo(&member, &w, "image/png", PNG.to_vec())
        .await
        .unwrap();
    assert_ne!(second["photo"], first["photo"]);
    assert!(
        db.member_photo(&owner, &w, &member.id, &first_id)
            .await
            .is_err(),
        "old address stops working"
    );
    assert!(
        db.save_member_photo(&member, &w, "image/png", vec![])
            .await
            .is_err()
    );
    assert!(
        db.save_member_photo(
            &member,
            &w,
            "image/png",
            vec![0; member_profiles::PHOTO_LIMIT + 1]
        )
        .await
        .is_err()
    );
    let removed = db.remove_member_photo(&member, &w).await.unwrap();
    assert!(removed["photo"].is_null());
    assert!(
        db.directory(&owner, &w).await.unwrap()["users"]
            .as_array()
            .unwrap()
            .iter()
            .all(|u| u["photo"].is_null())
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_profile_change_tells_open_screens_to_reload_people() {
    let db = fixture().await;
    let owner = user(&db, "profile-sync").await;
    let w = workspace(&db, &owner).await;
    let before = db.sync_page(&owner, &w, Some(0)).await.unwrap()["cursor"]
        .as_i64()
        .unwrap();
    db.save_member_profile(&owner, &w, "Priya", "Founder", "")
        .await
        .unwrap();
    let page = db.sync_page(&owner, &w, Some(before)).await.unwrap();
    assert!(
        page["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "access")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn leaving_the_workspace_removes_the_profile() {
    let db = fixture().await;
    let owner = user(&db, "profile-leave").await;
    let w = workspace(&db, &owner).await;
    let member = user(&db, "profile-leaver").await;
    join(&db, &owner, &w, &member, "Member").await;
    db.save_member_profile(&member, &w, "Arjun", "Sales", "")
        .await
        .unwrap();
    db.change(&owner, &w, &member.id, None, None).await.unwrap();
    assert!(
        db.save_member_profile(&member, &w, "Arjun", "Sales", "")
            .await
            .is_err()
    );
    let left = query(
        &db.db,
        "SELECT 1 AS n FROM relay_member_profiles WHERE user_id=$1",
        vec![member.id.clone().into()],
    )
    .await
    .unwrap();
    assert!(left.is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn channel_managers_add_and_remove_workspace_agents() {
    let db = fixture().await;
    let owner = user(&db, "agents-owner").await;
    let w = workspace(&db, &owner).await;
    let member = user(&db, "agents-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let (_, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let channel = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    let agents_in = |db: PgStore, viewer: User| {
        let (w, channel) = (w.clone(), channel.clone());
        async move {
            let chats = db.chats(&viewer, &w).await.unwrap();
            chats["chats"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == channel)
                .unwrap()["agents"]
                .clone()
        }
    };
    // A member who did not create the channel cannot change its agents.
    assert!(
        db.channel_agents(&member, &w, &channel, std::slice::from_ref(&agent))
            .await
            .is_err()
    );
    let before = db.sync_page(&member, &w, Some(0)).await.unwrap()["cursor"]
        .as_i64()
        .unwrap();
    db.channel_agents(&owner, &w, &channel, std::slice::from_ref(&agent))
        .await
        .unwrap();
    assert_eq!(
        agents_in(db.clone(), member.clone()).await,
        serde_json::json!([agent])
    );
    let page = db.sync_page(&member, &w, Some(before)).await.unwrap();
    assert!(
        page["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "access")
    );
    // Saving the same list again changes nothing; an empty list removes it.
    db.channel_agents(&owner, &w, &channel, std::slice::from_ref(&agent))
        .await
        .unwrap();
    db.channel_agents(&owner, &w, &channel, &[]).await.unwrap();
    assert_eq!(
        agents_in(db.clone(), member.clone()).await,
        serde_json::json!([])
    );
    assert!(
        db.channel_agents(&owner, &w, &channel, &["missing".into()])
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn private_agents_and_direct_messages_cannot_gain_channel_agents() {
    let db = fixture().await;
    let owner = user(&db, "private-agent-owner").await;
    let w = workspace(&db, &owner).await;
    let private = crate::identity::agents::config::SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Mine".into(),
        role: "Personal".into(),
        model: "fixture".into(),
        scope: "private".into(),
        revision: 0,
        settings: Default::default(),
    };
    db.save_agent_profile(&owner, &w, &private).await.unwrap();
    let channel = chat(&db, &owner, &w, vec![], vec![]).await;
    assert!(
        db.channel_agents(&owner, &w, &channel, std::slice::from_ref(&private.id))
            .await
            .is_err()
    );
    let (direct, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let _ = direct;
    let member = user(&db, "dm-partner").await;
    join(&db, &owner, &w, &member, "Member").await;
    let dm = db
        .chat_create(
            &owner,
            &w,
            &crate::identity::chat::model::NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "dm".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![member.id.clone()],
                agents: vec![],
            },
        )
        .await
        .unwrap();
    assert!(db.channel_agents(&owner, &w, &dm, &[agent]).await.is_err());
}
