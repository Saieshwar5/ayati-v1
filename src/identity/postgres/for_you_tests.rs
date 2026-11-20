//! For you: what is addressed to a person, read or not, and that it agrees with
//! the unread counts.
use super::super::{
    tests::{chat, fixture, join, message, user, workspace},
    *,
};
use crate::identity::chat::model::NewChat;
use serde_json::Value;

fn mention(person: &User, text: &str) -> String {
    format!("[@{}](mention:user:{}) {text}", person.id, person.id)
}

/// (preview, reason) of each item in one list of a page.
fn listed(page: &Value, list: &str) -> Vec<(String, String)> {
    page[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let text = item["content"].as_str().unwrap();
            let words = text.rsplit(") ").next().unwrap_or(text).to_owned();
            (words, item["reason"].as_str().unwrap().to_owned())
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn for_you_lists_mentions_agent_replies_and_followed_threads_in_channels() {
    let db = fixture().await;
    let owner = user(&db, "for-you-owner").await;
    let member = user(&db, "for-you-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let c = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    let direct = db
        .chat_create(
            &owner,
            &w,
            &NewChat {
                kind: "direct".into(),
                description: String::new(),
                id: uuid::Uuid::new_v4().to_string(),
                name: "Direct".into(),
                users: vec![member.id.clone()],
                agents: vec![],
            },
        )
        .await
        .unwrap();
    db.chat_send(&owner, &w, &c, &message("Hello everyone", None))
        .await
        .unwrap();
    let asked = message(&mention(&member, "please check the delivery"), None);
    db.chat_send(&owner, &w, &c, &asked).await.unwrap();
    db.chat_send(
        &owner,
        &w,
        &direct,
        &message(&mention(&member, "in a direct message"), None),
    )
    .await
    .unwrap();
    // An agent's answer to the member's request.
    execute(&db.db, "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,requester) VALUES($1,$2,$3,'agent-one','agent','Stock is fine','complete',$4,$5)",
        vec![uuid::Uuid::new_v4().to_string().into(), w.clone().into(), c.clone().into(), now().into(), member.id.clone().into()]).await.unwrap();
    // A thread the member started, answered by someone else.
    let question = message("My question", None);
    db.chat_send(&member, &w, &c, &question).await.unwrap();
    let mut answer = message("An answer in the thread", None);
    answer.thread = Some(question.id.clone());
    db.chat_send(&owner, &w, &c, &answer).await.unwrap();

    let page = db.for_you(&member, &w, None).await.unwrap();
    let mut new = listed(&page, "new");
    new.sort();
    assert_eq!(
        new,
        [
            ("An answer in the thread".to_owned(), "thread".to_owned()),
            ("Stock is fine".to_owned(), "agent_reply".to_owned()),
            ("please check the delivery".to_owned(), "mention".to_owned()),
        ],
        "not the chatter, not the direct message"
    );
    assert!(page["earlier"].as_array().unwrap().is_empty());
    assert_eq!(page["new"][0]["chat"], c.as_str());
    assert_eq!(page["new"][0]["chat_name"], "Operations");
    assert_eq!(page["new"][0]["unread"], true);

    // The For you count is the unread "for you" count of channels and groups.
    let counts = db.unread(&member, &w).await.unwrap();
    let mentions: i64 = counts["unread"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["chat"] != direct.as_str())
        .map(|item| item["mentions"].as_i64().unwrap())
        .sum();
    assert_eq!(mentions, 3);

    // Seeing it moves it to Earlier; nobody else sees the member's list.
    db.acknowledge(&member, &w, &c, std::slice::from_ref(&asked.id), true)
        .await
        .unwrap();
    let page = db.for_you(&member, &w, None).await.unwrap();
    assert_eq!(page["new"].as_array().unwrap().len(), 2);
    assert_eq!(
        listed(&page, "earlier"),
        [("please check the delivery".to_owned(), "mention".to_owned())]
    );
    assert_eq!(page["earlier"][0]["unread"], false);
    let theirs = db.for_you(&owner, &w, None).await.unwrap();
    assert!(theirs["new"].as_array().unwrap().is_empty());
    assert!(theirs["earlier"].as_array().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn read_items_come_in_pages_of_25() {
    let db = fixture().await;
    let owner = user(&db, "for-you-pages-owner").await;
    let member = user(&db, "for-you-pages-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let c = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    let mut ids = vec![];
    for n in 0..30 {
        let sent = message(&mention(&member, &format!("note {n}")), None);
        db.chat_send(&owner, &w, &c, &sent).await.unwrap();
        ids.push(sent.id);
    }
    db.acknowledge(&member, &w, &c, &ids, true).await.unwrap();

    let first = db.for_you(&member, &w, None).await.unwrap();
    assert!(first["new"].as_array().unwrap().is_empty());
    assert_eq!(first["earlier"].as_array().unwrap().len(), 25);
    let before = first["next_before"].as_str().unwrap().to_owned();
    let second = db.for_you(&member, &w, Some(&before)).await.unwrap();
    assert!(
        second["new"].as_array().unwrap().is_empty(),
        "only the first page has new items"
    );
    assert_eq!(second["earlier"].as_array().unwrap().len(), 5);
    assert!(second["next_before"].is_null());
    let mut seen: Vec<_> = [&first, &second]
        .iter()
        .flat_map(|page| page["earlier"].as_array().unwrap().clone())
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 30, "every item once");
    assert!(db.for_you(&member, &w, Some("not-a-cursor")).await.is_err());
}
