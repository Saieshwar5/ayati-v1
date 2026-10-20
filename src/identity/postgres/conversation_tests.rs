use super::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn channel_membership_and_concurrent_direct_creation() {
    let db = fixture().await;
    let owner = user(&db, "channel-owner").await;
    let member = user(&db, "channel-member").await;
    let outsider = user(&db, "channel-outsider").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let mut input = NewChat {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Inventory".into(),
        kind: "channel".into(),
        description: "Stock".into(),
        users: vec![],
        agents: vec![],
    };
    assert!(db.chat_create(&owner, &w, &input).await.is_err());
    input.users = vec![member.id.clone()];
    let channel = db.chat_create(&owner, &w, &input).await.unwrap();
    assert!(db.chat_messages(&outsider, &w, &channel).await.is_err());
    assert_eq!(
        db.chats(&member, &w).await.unwrap()["chats"][0]["kind"],
        "channel"
    );
    input.id = uuid::Uuid::new_v4().to_string();
    input.kind = "direct".into();
    let reverse = NewChat {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Owner".into(),
        kind: "direct".into(),
        description: String::new(),
        users: vec![owner.id.clone()],
        agents: vec![],
    };
    let (a, b) = tokio::join!(
        db.chat_create(&owner, &w, &input),
        db.chat_create(&member, &w, &reverse)
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(
        db.chats(&owner, &w).await.unwrap()["chats"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn threads_do_not_cross_conversations_or_leak_context() {
    let db = fixture().await;
    let owner = user(&db, "thread-owner").await;
    let w = workspace(&db, &owner).await;
    let first = chat(&db, &owner, &w, vec![], vec!["local-agent".into()]).await;
    let second = chat(&db, &owner, &w, vec![], vec![]).await;
    db.chat_send(&owner, &w, &first, &message("OTHER-THREAD-SECRET", None))
        .await
        .unwrap();
    let root = message("Check stock", None);
    db.chat_send(&owner, &w, &first, &root).await.unwrap();
    let mut reply = message("Review stock", Some("local-agent"));
    reply.thread = Some(root.id.clone());
    assert!(db.chat_send(&owner, &w, &second, &reply).await.is_err());
    let job = db
        .chat_send(&owner, &w, &first, &reply)
        .await
        .unwrap()
        .unwrap();
    assert!(!job.prompt.contains("OTHER-THREAD-SECRET"));
    assert!(job.prompt.contains("Check stock"));
    assert!(
        db.chat_send(&owner, &w, &first, &reply)
            .await
            .unwrap()
            .is_none()
    );
    let history = db.chat_messages(&owner, &w, &first).await.unwrap();
    assert_eq!(history["messages"][3]["thread"], root.id);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agent_direct_messages_keep_context_and_reply_in_the_timeline() {
    let db = fixture().await;
    let owner = user(&db, "direct-owner").await;
    let w = workspace(&db, &owner).await;
    let input = NewChat {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Assistant".into(),
        kind: "direct".into(),
        description: String::new(),
        users: vec![],
        agents: vec!["local-agent".into()],
    };
    let direct = db.chat_create(&owner, &w, &input).await.unwrap();
    db.chat_send(
        &owner,
        &w,
        &direct,
        &message("Remember the inventory topic", None),
    )
    .await
    .unwrap();
    let job = db
        .chat_send(
            &owner,
            &w,
            &direct,
            &message("Continue", Some("local-agent")),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(job.prompt.contains("Remember the inventory topic"));
    assert!(
        db.chat_messages(&owner, &w, &direct).await.unwrap()["messages"][2]["thread"].is_null()
    );
}
