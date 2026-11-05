//! Library LB2a: agents search passages by section, read whole sections, cite the
//! section they read, and anything they were shown fences their reply.
use super::super::{tests::*, *};
use super::folders::{ask, grant, page, setup, share};
use crate::identity::control::broker::invoke;
use crate::identity::library::model::*;
use serde_json::json;

fn heading(text: &str) -> Value {
    json!({"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":text}]})
}
fn paragraph(text: &str) -> Value {
    json!({"type":"paragraph","content":[{"type":"text","text":text}]})
}
fn handbook(id: &str, revision: i64, days: u32) -> SavePage {
    SavePage {
        id: id.into(),
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        title: "Support handbook".into(),
        document: json!({"type":"doc","content":[
            heading("Shipping"), paragraph("Orders leave the warehouse within two working days."),
            heading("Refunds"), paragraph(&format!("Customers may send items back within {days} days and are refunded to the card they paid with.")),
            heading("Opening hours"), paragraph("The shop is open from nine to five.")]}),
        folder: None,
    }
}
async fn content(db: &PgStore, reply: &str) -> (String, String) {
    let row = query(
        &db.db,
        "SELECT content,state FROM relay_chat_messages WHERE id=$1",
        vec![reply.into()],
    )
    .await
    .unwrap()
    .unwrap();
    (
        row.try_get("", "content").unwrap(),
        row.try_get("", "state").unwrap(),
    )
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_find_the_section_that_answers_and_cite_it() {
    let s = setup().await;
    let db = &s.db;
    let policies = db.library_get(&s.owner, &s.w, &s.returns).await.unwrap()["folder"]
        .as_str()
        .unwrap()
        .to_owned();
    let id = uuid::Uuid::new_v4().to_string();
    let mut book = handbook(&id, 0, 30);
    book.folder = Some(policies.clone());
    db.library_save(&s.owner, &s.w, &book).await.unwrap();
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(
        &s,
        &s.member,
        &theirs,
        "How long do customers have to return an item?",
    )
    .await;
    // Words are matched by their stem ("refunded" finds "refund"), across any of them.
    let found = invoke(
        db,
        &job,
        "search_library",
        json!({"query":"how long until items are refunded?"}),
    )
    .await
    .unwrap();
    let top = &found["results"][0];
    assert_eq!(
        (top["title"].as_str(), top["section"].as_str()),
        (Some("Support handbook"), Some("Refunds"))
    );
    assert!(top["preview"].as_str().unwrap().contains("**refunded**"));
    assert!(top.get("access_revision").is_none());
    assert!(
        found["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["title"] != "Salaries"),
        "the HR folder is not searched for a colleague"
    );
    let within = invoke(
        db,
        &job,
        "search_library",
        json!({"query":"refund","folder":policies}),
    )
    .await
    .unwrap();
    assert!(!within["results"].as_array().unwrap().is_empty());
    // Reading from that passage returns whole sections; the first read lists them.
    let start = invoke(db, &job, "read_document", json!({"id":id}))
        .await
        .unwrap();
    assert_eq!(
        start["outline"],
        json!([{"passage":0,"section":"Shipping"},{"passage":1,"section":"Refunds"},{"passage":2,"section":"Opening hours"}])
    );
    let read = invoke(
        db,
        &job,
        "read_document",
        json!({"id":id,"passage":top["passage"]}),
    )
    .await
    .unwrap();
    assert_eq!(read["passages"][0]["section"], "Refunds");
    assert!(
        read["passages"][0]["text"]
            .as_str()
            .unwrap()
            .contains("within 30 days")
    );
    assert!(read["outline"].is_null());
    assert!(
        invoke(db, &job, "read_document", json!({"id":id,"passage":9}))
            .await
            .is_err()
    );
    db.chat_finish(&job, "30 days.", "complete").await.unwrap();
    let (answer, _) = content(db, &job.reply).await;
    assert!(answer.contains(&format!("[Support handbook › Refunds · revision 1](/w/{}/?view=library&item={id}&source_revision=1)", s.w)));
    assert!(
        !answer.contains("Returns policy"),
        "documents only previewed by search aren't cited"
    );
    // An edit re-indexes: the next answer reads the new text at the new revision.
    db.library_save(
        &s.owner,
        &s.w,
        &SavePage {
            folder: None,
            ..handbook(&id, 1, 45)
        },
    )
    .await
    .unwrap();
    let next = ask(&s, &s.member, &theirs, "And now?").await;
    let found = invoke(db, &next, "search_library", json!({"query":"refunded"}))
        .await
        .unwrap();
    assert_eq!(found["results"][0]["revision"], 2);
    let read = invoke(
        db,
        &next,
        "read_document",
        json!({"id":id,"passage":found["results"][0]["passage"]}),
    )
    .await
    .unwrap();
    assert!(
        read["passages"][0]["text"]
            .as_str()
            .unwrap()
            .contains("within 45 days")
    );
    db.chat_finish(&next, "45 days.", "complete").await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn older_items_are_indexed_when_agents_search_and_files_name_their_sheets() {
    let s = setup().await;
    let db = &s.db;
    // A file from before passages existed: no passages, never indexed.
    let file = uuid::Uuid::new_v4().to_string();
    let policies = db.library_get(&s.owner, &s.w, &s.returns).await.unwrap()["folder"]
        .as_str()
        .unwrap()
        .to_owned();
    execute(&db.db, "INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,text,mime,readiness,created,updated,folder) VALUES($1,$2,'file','Price list.xlsx',$3,$3,$4,$5,'ready',1,1,$6)",
        vec![s.w.clone().into(), file.clone().into(), s.owner.id.clone().into(), "\n# Price list\nProduct | Price\nProduct: Standing desk · Price: 1250\n".into(), "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into(), policies.into()])
        .await
        .unwrap();
    execute(
        &db.db,
        "DELETE FROM relay_library_passages WHERE workspace=$1",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap();
    execute(
        &db.db,
        "DELETE FROM relay_library_indexed WHERE workspace=$1",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap();
    let fenced: i64 = query(&db.db, "SELECT COALESCE(sum(revision),0)::bigint AS n FROM relay_context_epochs WHERE workspace=$1", vec![s.w.clone().into()])
        .await.unwrap().unwrap().try_get("", "n").unwrap();
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(&s, &s.member, &theirs, "What does the standing desk cost?").await;
    let found = invoke(
        db,
        &job,
        "search_library",
        json!({"query":"standing desk price"}),
    )
    .await
    .unwrap();
    assert_eq!(found["results"][0]["title"], "Price list.xlsx");
    assert_eq!(found["results"][0]["section"], "Price list");
    let refund = invoke(db, &job, "search_library", json!({"query":"refund window"}))
        .await
        .unwrap();
    assert_eq!(
        refund["results"][0]["title"], "Returns policy",
        "the older page was caught up too"
    );
    let stale: i64 = query(&db.db, "SELECT count(*) AS n FROM relay_library_items i LEFT JOIN relay_library_indexed x ON x.workspace=i.workspace AND x.item=i.id WHERE i.workspace=$1 AND i.kind<>'folder' AND x.revision IS DISTINCT FROM i.revision", vec![s.w.clone().into()])
        .await.unwrap().unwrap().try_get("", "n").unwrap();
    assert_eq!(stale, 0);
    let after: i64 = query(&db.db, "SELECT COALESCE(sum(revision),0)::bigint AS n FROM relay_context_epochs WHERE workspace=$1", vec![s.w.clone().into()])
        .await.unwrap().unwrap().try_get("", "n").unwrap();
    assert_eq!(
        after, fenced,
        "indexing is not an access change: earlier answers stay visible"
    );
    db.chat_finish(&job, "1250.", "complete").await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn what_search_showed_fences_the_reply_even_when_never_read() {
    let s = setup().await;
    let db = &s.db;
    let private = chat(db, &s.owner, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(&s, &s.owner, &private, "What is the support salary band?").await;
    let found = invoke(db, &job, "search_library", json!({"query":"salary band"}))
        .await
        .unwrap();
    assert_eq!(found["results"][0]["title"], "Salaries");
    assert!(
        found["results"][0]["preview"]
            .as_str()
            .unwrap()
            .contains("40k")
    );
    assert!(db.chat_job_allowed(&job).await);
    db.library_share(
        &s.owner,
        &s.w,
        &s.hr,
        &share(2, "private", vec![], vec![grant(&s.agent, "staff")]),
    )
    .await
    .unwrap();
    assert!(
        !db.chat_job_allowed(&job).await,
        "the preview's sharing changed"
    );
    db.chat_finish(&job, "The band is 40k.", "complete")
        .await
        .unwrap();
    let (answer, state) = content(db, &job.reply).await;
    assert_eq!(state, "cancelled");
    assert!(!answer.contains("40k"));
    // A new page is searchable as soon as it is saved.
    let fresh = page("Holiday rota", "Ravi covers the Diwali week.", None);
    db.library_save(&s.owner, &s.w, &fresh).await.unwrap();
    db.library_share(
        &s.owner,
        &s.w,
        &fresh.id,
        &share(1, "private", vec![], vec![grant(&s.agent, "staff")]),
    )
    .await
    .unwrap();
    let later = ask(&s, &s.owner, &private, "Who covers Diwali?").await;
    let found = invoke(db, &later, "search_library", json!({"query":"Diwali"}))
        .await
        .unwrap();
    assert_eq!(found["results"][0]["title"], "Holiday rota");
}
