//! Message search against real PostgreSQL: words, prefixes and scripts, newest
//! first, the same access as reading, and filters.
use super::message_search::SearchInput;
use super::tests::{chat, fixture, join, message, user, workspace};
use super::*;
use serde_json::json;

fn q(text: &str) -> SearchInput {
    SearchInput {
        q: text.into(),
        ..Default::default()
    }
}

fn ids(page: &Value) -> Vec<String> {
    page["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn search_finds_words_and_prefixes_newest_first_only_where_people_can_read() {
    let db = fixture().await;
    let owner = user(&db, "search-owner").await;
    let member = user(&db, "search-member").await;
    let outsider = user(&db, "search-outsider").await;
    let w = workspace(&db, &owner).await;
    let other = workspace(&db, &outsider).await;
    join(&db, &owner, &w, &member, "Member").await;
    // A private group with both, a private one with the owner only, and a public channel.
    let shared = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    let private = chat(&db, &owner, &w, vec![], vec![]).await;
    let public = chat(&db, &owner, &w, vec![], vec![]).await;
    execute(
        &db.db,
        "UPDATE relay_chats SET visibility='public' WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), public.clone().into()],
    )
    .await
    .unwrap();
    let first = message("The delivery date for Mehta moved to Friday", None);
    let second = message(
        &format!(
            "Ask [@Search Member](mention:user:{}) about the **delivery** invoice",
            owner.id
        ),
        None,
    );
    let secret = message("Delivery plan for the board only", None);
    let open = message("Delivery numbers for everyone", None);
    let hindi = message("नमस्ते टीम, डिलीवरी कल होगी", None);
    db.chat_send(&owner, &w, &shared, &first).await.unwrap();
    db.chat_send(&member, &w, &shared, &second).await.unwrap();
    db.chat_send(&owner, &w, &private, &secret).await.unwrap();
    db.chat_send(&owner, &w, &public, &open).await.unwrap();
    db.chat_send(&owner, &w, &shared, &hindi).await.unwrap();

    // A partly typed word finds every visible message containing it, newest first.
    let page = db.search_messages(&member, &w, &q("deliv")).await.unwrap();
    assert_eq!(
        ids(&page),
        [open.id.clone(), second.id.clone(), first.id.clone()],
        "the owner-only chat is never searched"
    );
    let hit = &page["results"][1];
    assert_eq!(
        hit["snippet"],
        "Ask @Search Member about the delivery invoice"
    );
    assert_eq!(
        hit["name"], "search-member@example.test",
        "named as in history"
    );
    assert_eq!(hit["chat"], shared.as_str());
    assert_eq!(page["terms"], json!(["deliv"]));
    // Every word must appear; case and punctuation don't matter.
    assert_eq!(
        ids(&db
            .search_messages(&member, &w, &q("DELIVERY, friday?"))
            .await
            .unwrap()),
        [first.id.as_str()]
    );
    assert_eq!(
        ids(&db.search_messages(&member, &w, &q("नमस्ते")).await.unwrap()),
        [hindi.id.as_str()]
    );
    // The owner also sees their own private chat; another workspace sees nothing.
    assert_eq!(
        db.search_messages(&owner, &w, &q("delivery"))
            .await
            .unwrap()["results"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(
        db.search_messages(&outsider, &w, &q("delivery"))
            .await
            .is_err()
    );
    assert!(
        ids(&db
            .search_messages(&outsider, &other, &q("delivery"))
            .await
            .unwrap())
        .is_empty()
    );
    // Searching one conversation, one author, or a conversation one can't read.
    let mut input = q("delivery");
    input.chat = Some(shared.clone());
    assert_eq!(
        ids(&db.search_messages(&member, &w, &input).await.unwrap()),
        [second.id.clone(), first.id.clone()]
    );
    input.from = Some(member.id.clone());
    assert_eq!(
        ids(&db.search_messages(&member, &w, &input).await.unwrap()),
        [second.id.as_str()]
    );
    input.chat = Some(private.clone());
    assert!(db.search_messages(&member, &w, &input).await.is_err());
    // Nothing to search for.
    assert!(db.search_messages(&member, &w, &q("  &&  ")).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn edits_and_deletions_change_what_search_finds() {
    let db = fixture().await;
    let owner = user(&db, "search-editor").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let m = message("Quarterly forecast draft", None);
    db.chat_send(&owner, &w, &c, &m).await.unwrap();
    db.edit_message(&owner, &w, &c, &m.id, "Quarterly budget final")
        .await
        .unwrap();
    assert!(
        ids(&db
            .search_messages(&owner, &w, &q("forecast"))
            .await
            .unwrap())
        .is_empty()
    );
    assert_eq!(
        ids(&db.search_messages(&owner, &w, &q("budget")).await.unwrap()),
        [m.id.as_str()]
    );
    db.delete_message(&owner, &w, &c, &m.id).await.unwrap();
    assert!(
        ids(&db
            .search_messages(&owner, &w, &q("quarterly"))
            .await
            .unwrap())
        .is_empty()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn results_page_from_newest_to_oldest() {
    let db = fixture().await;
    let owner = user(&db, "search-pager").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let mut sent = Vec::new();
    for n in 0..35 {
        let m = message(&format!("standup note {n}"), None);
        db.chat_send(&owner, &w, &c, &m).await.unwrap();
        sent.push(m.id);
    }
    let first = db.search_messages(&owner, &w, &q("standup")).await.unwrap();
    assert_eq!(first["results"].as_array().unwrap().len(), 30);
    assert_eq!(first["results"][0]["id"], sent[34].as_str());
    let mut next = q("standup");
    next.before = first["next_before"].as_i64();
    assert!(next.before.is_some());
    let second = db.search_messages(&owner, &w, &next).await.unwrap();
    assert_eq!(
        ids(&second),
        sent[..5].iter().rev().cloned().collect::<Vec<_>>()
    );
    assert_eq!(second["next_before"], Value::Null);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn filters_narrow_to_mentions_of_me_agents_links_and_threads() {
    let db = fixture().await;
    let owner = user(&db, "filter-owner").await;
    let member = user(&db, "filter-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let (_, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let c = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec![agent.clone()],
    )
    .await;
    let plain = message("Supplier sent the price list", None);
    let mention = message(
        &format!("[@Member](mention:user:{}) supplier call at 3", member.id),
        None,
    );
    let link = message("Supplier page https://shree.example/prices", None);
    db.chat_send(&owner, &w, &c, &plain).await.unwrap();
    db.chat_send(&owner, &w, &c, &mention).await.unwrap();
    db.chat_send(&owner, &w, &c, &link).await.unwrap();
    let reply = crate::identity::chat::model::NewMessage {
        thread: Some(plain.id.clone()),
        ..message("Supplier confirmed in the thread", None)
    };
    db.chat_send(&member, &w, &c, &reply).await.unwrap();
    // An agent's finished reply that names the supplier.
    let asked = message("Summarise the supplier terms", None);
    db.chat_send(
        &owner,
        &w,
        &c,
        &crate::identity::chat::model::NewMessage {
            agents: vec![agent.clone()],
            ..asked
        },
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "Supplier terms: 30 days", "complete")
        .await
        .unwrap();

    let find = |input: SearchInput| {
        let db = &db;
        let member = &member;
        let w = &w;
        async move { ids(&db.search_messages(member, w, &input).await.unwrap()) }
    };
    let base = || q("supplier");
    assert_eq!(
        find(SearchInput {
            mentions_me: true,
            ..base()
        })
        .await,
        [mention.id.clone()]
    );
    assert_eq!(
        find(SearchInput {
            agents: true,
            ..base()
        })
        .await,
        [job.reply.clone()]
    );
    assert_eq!(
        find(SearchInput {
            links: true,
            ..base()
        })
        .await,
        [link.id.clone()]
    );
    assert_eq!(
        find(SearchInput {
            threads: true,
            ..base()
        })
        .await,
        [reply.id.clone()]
    );
    // From me is the existing author filter with my own id.
    assert_eq!(
        find(SearchInput {
            from: Some(member.id.clone()),
            ..base()
        })
        .await,
        [reply.id.clone()]
    );
    // Filters combine: a mention of me that is also a thread reply has none here.
    assert!(
        find(SearchInput {
            mentions_me: true,
            threads: true,
            ..base()
        })
        .await
        .is_empty()
    );
}
