//! Link previews only for messages the reader may read, and a cache that
//! keeps an answer a day and a miss an hour.
use super::tests::*;
use super::*;
use crate::identity::chat::link_preview::Preview;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn previews_are_for_readable_peoples_messages_and_are_cached() {
    let db = fixture().await;
    let owner = user(&db, "preview-owner").await;
    let w = workspace(&db, &owner).await;
    let outsider = user(&db, "preview-outsider").await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let linked = message("Prices at https://shree.example/prices.", None);
    let plain = message("No link", None);
    db.chat_send(&owner, &w, &c, &linked).await.unwrap();
    db.chat_send(&owner, &w, &c, &plain).await.unwrap();

    assert_eq!(
        db.message_link(&owner, &w, &c, &linked.id)
            .await
            .unwrap()
            .as_deref(),
        Some("https://shree.example/prices")
    );
    assert_eq!(
        db.message_link(&owner, &w, &c, &plain.id).await.unwrap(),
        None
    );
    assert!(
        db.message_link(&outsider, &w, &c, &linked.id)
            .await
            .is_err(),
        "no access, no link"
    );
    db.delete_message(&owner, &w, &c, &linked.id).await.unwrap();
    assert_eq!(
        db.message_link(&owner, &w, &c, &linked.id).await.unwrap(),
        None
    );

    let url = "https://shree.example/prices";
    assert_eq!(db.cached_preview(url).await.unwrap(), None);
    let preview = Preview {
        url: url.into(),
        title: "Prices 2027".into(),
        site: "Shree".into(),
        description: None,
    };
    db.save_preview(url, Some(&preview)).await.unwrap();
    assert_eq!(
        db.cached_preview(url).await.unwrap().unwrap()["title"],
        "Prices 2027"
    );
    // A page without a preview is remembered for an hour, then tried again.
    db.save_preview(url, None).await.unwrap();
    assert_eq!(db.cached_preview(url).await.unwrap(), Some(Value::Null));
    execute(
        &db.db,
        "UPDATE relay_link_previews SET fetched=fetched-3601 WHERE url=$1",
        vec![url.into()],
    )
    .await
    .unwrap();
    assert_eq!(db.cached_preview(url).await.unwrap(), None);
    db.save_preview(url, Some(&preview)).await.unwrap();
    execute(
        &db.db,
        "UPDATE relay_link_previews SET fetched=fetched-3601 WHERE url=$1",
        vec![url.into()],
    )
    .await
    .unwrap();
    assert!(
        db.cached_preview(url).await.unwrap().is_some(),
        "a preview lasts a day"
    );
}
