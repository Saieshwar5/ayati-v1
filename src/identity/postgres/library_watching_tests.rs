//! Library LB6: what arrives in a watched folder becomes agent work for the watcher;
//! the agent records key fields and files the item, only where it may.
use super::super::*;
use super::folders::{folder, page, setup, share};
use crate::identity::control::broker::invoke;
use crate::identity::library::model::{AgentGrant, MoveItem};
use crate::identity::postgres::Watch;
use serde_json::json;

/// Claims the background job started for an arrival, as an executor would.
async fn claim(db: &PgStore, item: &str) -> crate::identity::chat::model::Job {
    let row = query(&db.db, "SELECT j.payload FROM relay_chat_jobs j JOIN relay_library_arrivals a ON a.workspace=j.workspace AND a.reply=j.reply WHERE a.item=$1", vec![item.into()]).await.unwrap().unwrap();
    let mut job: crate::identity::chat::model::Job =
        serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap();
    job.claim = Some(uuid::Uuid::new_v4().to_string());
    execute(&db.db, "UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1", vec![job.reply.clone().into(), job.claim.clone().into()]).await.unwrap();
    job
}
fn grant(agent: &str, level: &str) -> AgentGrant {
    AgentGrant {
        agent: agent.into(),
        level: level.into(),
        use_for: "staff".into(),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn new_invoices_are_read_recorded_and_filed_by_the_watching_agent() {
    let mut s = setup().await;
    s.db.pooled = true;
    let db = &s.db;
    let inbox = folder(db, &s.owner, &s.w, "Inbox").await;
    let invoices = folder(db, &s.owner, &s.w, "Invoices").await;
    db.library_share(
        &s.owner,
        &s.w,
        &inbox,
        &share(1, "private", vec![], vec![grant(&s.agent, "suggest")]),
    )
    .await
    .unwrap();
    db.library_share(
        &s.owner,
        &s.w,
        &invoices,
        &share(1, "private", vec![], vec![grant(&s.agent, "save")]),
    )
    .await
    .unwrap();
    let watch = |agent: Option<&str>| Watch {
        agent: agent.map(str::to_owned),
        instructions:
            "Record the amount and due date, file it under Invoices, and ask me whether to pay."
                .into(),
    };
    // Only the folder's manager sets a watcher, and only an agent given the folder.
    assert!(
        db.library_watch_set(&s.member, &s.w, &inbox, &watch(Some(&s.agent)))
            .await
            .is_err()
    );
    let other = folder(db, &s.owner, &s.w, "Receipts").await;
    let refused = db
        .library_watch_set(&s.owner, &s.w, &other, &watch(Some(&s.agent)))
        .await
        .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("Give the agent this folder first"),
        "{refused}"
    );
    let set = db
        .library_watch_set(&s.owner, &s.w, &inbox, &watch(Some(&s.agent)))
        .await
        .unwrap();
    let chat_id = set["chat"].as_str().unwrap().to_owned();
    assert_eq!(
        db.library_watch(&s.owner, &s.w, &inbox).await.unwrap()["watcher"]["chat"],
        chat_id.as_str()
    );
    // An invoice saved into the inbox is queued once; one saved elsewhere isn't.
    let invoice = page(
        "Invoice 42",
        "Acme Ltd. Amount due 1,250. Pay by 1 December 2026.",
        Some(&inbox),
    );
    db.library_save(&s.owner, &s.w, &invoice).await.unwrap();
    db.library_save(&s.owner, &s.w, &page("Note", "Not an invoice.", None))
        .await
        .unwrap();
    assert_eq!(db.dispatch_library_arrivals().await.unwrap(), 1);
    assert_eq!(
        db.dispatch_library_arrivals().await.unwrap(),
        0,
        "started once"
    );
    let job = claim(db, &invoice.id).await;
    assert_eq!(job.chat, chat_id);
    assert_eq!(job.requester, s.owner.id);
    assert!(
        job.prompt
            .contains("New in the Library folder \\\"Inbox\\\": Invoice 42"),
        "{}",
        job.prompt
    );
    assert!(job.prompt.contains(&invoice.id));
    // While it works, the next arrival in the same folder waits its turn.
    let next = page("Invoice 43", "Globex. Amount due 90.", Some(&inbox));
    db.library_save(&s.owner, &s.w, &next).await.unwrap();
    assert_eq!(db.dispatch_library_arrivals().await.unwrap(), 0);
    // The agent records key fields and files the invoice; it can't file into HR.
    let read = invoke(db, &job, "read_document", json!({"id":invoice.id}))
        .await
        .unwrap();
    assert_eq!(read["state"], "available");
    let fields =
        json!({"id":invoice.id,"fields":{"supplier":"Acme Ltd","amount":1250,"due":"2026-12-01"}});
    assert_eq!(
        invoke(db, &job, "set_document_fields", fields)
            .await
            .unwrap()["state"],
        "saved"
    );
    let again = invoke(db, &job, "read_document", json!({"id":invoice.id}))
        .await
        .unwrap();
    assert_eq!(again["fields"]["due"], "2026-12-01");
    assert_eq!(
        invoke(
            db,
            &job,
            "file_document",
            json!({"id":invoice.id,"folder":s.hr})
        )
        .await
        .unwrap()["state"],
        "not_allowed"
    );
    assert_eq!(
        invoke(
            db,
            &job,
            "file_document",
            json!({"id":invoice.id,"folder":inbox})
        )
        .await
        .unwrap()["state"],
        "not_allowed",
        "suggest isn't save"
    );
    assert_eq!(
        invoke(
            db,
            &job,
            "file_document",
            json!({"id":invoice.id,"folder":invoices})
        )
        .await
        .unwrap()["state"],
        "filed"
    );
    let after = invoke(db, &job, "read_document", json!({"id":invoice.id}))
        .await
        .unwrap();
    assert_eq!(
        after["state"], "available",
        "its own filing keeps the read valid"
    );
    let filed = db.library_get(&s.owner, &s.w, &invoice.id).await.unwrap();
    assert_eq!(filed["folder"], invoices.as_str());
    let shown = db
        .library_fields(&s.owner, &s.w, &invoice.id)
        .await
        .unwrap();
    assert_eq!(shown["fields"]["fields"]["amount"], 1250);
    assert!(
        invoke(
            db,
            &job,
            "set_document_fields",
            json!({"id":invoice.id,"fields":{"notes":"x".repeat(201)}})
        )
        .await
        .is_err()
    );
    db.chat_finish(
        &job,
        "Filed Invoice 42: 1,250 due 1 December. Pay it?",
        "complete",
    )
    .await
    .unwrap();
    // Then the next one starts.
    assert_eq!(db.dispatch_library_arrivals().await.unwrap(), 1);
    let second = claim(db, &next.id).await;
    assert!(second.prompt.contains("Invoice 43"));
    db.chat_finish(&second, "Done.", "complete").await.unwrap();
    // Stopping skips anything still waiting; moving an item in counts as arriving.
    let late = page("Invoice 44", "Initech. Amount due 10.", None);
    db.library_save(&s.owner, &s.w, &late).await.unwrap();
    db.library_move(
        &s.owner,
        &s.w,
        &late.id,
        &MoveItem {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            folder: Some(inbox.clone()),
        },
    )
    .await
    .unwrap();
    db.library_watch_set(&s.owner, &s.w, &inbox, &watch(None))
        .await
        .unwrap();
    assert_eq!(db.dispatch_library_arrivals().await.unwrap(), 0);
    let state: String = rows(
        &db.db,
        "SELECT state FROM relay_library_arrivals WHERE item=$1",
        vec![late.id.clone().into()],
    )
    .await
    .unwrap()[0]
        .try_get("", "state")
        .unwrap();
    assert_eq!(state, "skipped");
    let said: Vec<String> = rows(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND summary IS NOT NULL",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.try_get("", "summary").unwrap())
    .collect();
    let said = said.join("\n");
    for line in [
        "to handle new items in \"Inbox\"",
        "recorded supplier, amount, due on \"Invoice 42\"",
        "filed \"Invoice 42\" in \"Invoices\"",
        "Stopped watching \"Inbox\"",
    ] {
        assert!(said.contains(line), "missing {line}\n{said}");
    }
}
