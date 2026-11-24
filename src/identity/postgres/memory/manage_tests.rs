//! Memory (schema 142) as people see it: admins open the workspace's memory,
//! an agent's owners its job and room notes, each person only what an agent
//! remembers about them; a correction from a private chat waits for an admin.
use super::super::tests::*;
use super::manage::Page;
use super::tests::*;
use super::*;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn admins_owners_and_people_see_and_change_only_their_part() {
    let db = fixture().await;
    let p = place(&db).await;
    let job = ask(
        &db,
        &p.meera,
        &p,
        &p.meera_chat,
        "Remember: short answers. Acme pays after 45 days.",
    )
    .await;
    tool(
        &db,
        &job,
        "remember",
        json!({"text":"Meera wants short answers.","kind":"preference"}),
    )
    .await;
    tool(&db, &job, "remember", json!({"text":"Acme Traders pays 45 days after invoice.","kind":"customer","topic":"Acme Traders","for":"workspace"})).await;
    tool(
        &db,
        &job,
        "search_memory",
        json!({"query":"Northwind delivery days"}),
    )
    .await;
    done(&db, &job).await;
    let job = ask(&db, &p.owner, &p, &p.channel, "Notes for the team").await;
    tool(&db, &job, "remember", json!({"text":"The invoice export fails when a code has a slash.","kind":"system","for":"my_job"})).await;
    tool(
        &db,
        &job,
        "remember",
        json!({"text":"This channel quotes prices in rupees only.","kind":"decision"}),
    )
    .await;
    tool(
        &db,
        &job,
        "search_memory",
        json!({"query":"refund approver"}),
    )
    .await;
    done(&db, &job).await;

    // Only admins open the workspace's memory.
    assert!(
        db.memory_page(&p.meera, &p.w, Page::Workspace, "notes", None, None)
            .await
            .is_err()
    );
    let waiting = db
        .memory_page(&p.owner, &p.w, Page::Workspace, "waiting", None, None)
        .await
        .unwrap();
    assert_eq!(
        texts(&waiting, "items"),
        ["Acme Traders pays 45 days after invoice."]
    );
    assert_eq!(
        waiting["items"][0]["learned_in"],
        Value::Null,
        "the admin can't open Meera's chat, so it is not named"
    );
    assert_eq!(waiting["counts"]["waiting"], 1);
    let acme = id_of(&db, "Acme Traders pays 45 days after invoice.").await;
    db.change_memory_note(&p.owner, &p.w, Page::Workspace, &acme, "keep")
        .await
        .unwrap();
    let notes = db
        .memory_page(&p.owner, &p.w, Page::Workspace, "notes", None, None)
        .await
        .unwrap();
    assert_eq!(
        texts(&notes, "items"),
        ["Acme Traders pays 45 days after invoice."]
    );
    assert!(notes["items"][0]["strength"].as_i64().unwrap() >= 90);
    let topics = db
        .memory_page(&p.owner, &p.w, Page::Workspace, "topics", None, None)
        .await
        .unwrap();
    assert_eq!(topics["items"][0]["topic"], "Acme Traders");
    // What agents looked for and did not find: only from conversations the admin can open.
    let missed = db
        .memory_page(&p.owner, &p.w, Page::Workspace, "missed", None, None)
        .await
        .unwrap();
    let asked: Vec<&str> = missed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["query"].as_str().unwrap())
        .collect();
    assert_eq!(asked, ["refund approver"]);

    // An agent's own page is for its owners and admins: job and room notes, never a person's.
    assert!(
        db.memory_page(&p.meera, &p.w, Page::Agent(&p.a), "notes", None, None)
            .await
            .is_err()
    );
    let agent = db
        .memory_page(&p.owner, &p.w, Page::Agent(&p.a), "notes", None, None)
        .await
        .unwrap();
    let mut seen = texts(&agent, "items");
    seen.sort();
    assert_eq!(
        seen,
        [
            "The invoice export fails when a code has a slash.",
            "This channel quotes prices in rupees only."
        ]
    );

    // About you: Meera alone sees and removes hers.
    let mine = db.memory_about_me(&p.meera, &p.w, &p.a).await.unwrap();
    assert_eq!(texts(&mine, "notes"), ["Meera wants short answers."]);
    assert_eq!(
        db.memory_about_me(&p.ravi, &p.w, &p.a).await.unwrap()["notes"],
        json!([])
    );
    let hers = mine["notes"][0]["id"].as_str().unwrap().to_owned();
    assert!(
        db.forget_about_me(&p.ravi, &p.w, &p.a, &hers)
            .await
            .is_err()
    );
    db.forget_about_me(&p.meera, &p.w, &p.a, &hers)
        .await
        .unwrap();
    assert_eq!(
        db.memory_about_me(&p.meera, &p.w, &p.a).await.unwrap()["notes"],
        json!([])
    );

    // From a private chat an agent can't end a workspace note; its correction waits for an admin.
    let job = ask(&db, &p.ravi, &p, &p.ravi_chat, "Acme moved to 60 days").await;
    let name = &acme[..8];
    let ended = tool(&db, &job, "end_memory", json!({"memory":name})).await;
    assert_eq!(ended["state"], "refused");
    let fix = tool(&db, &job, "remember", json!({"text":"Acme Traders pays 60 days after invoice.","kind":"customer","replaces":name})).await;
    assert_eq!(fix["state"], "waiting");
    done(&db, &job).await;
    let waiting = db
        .memory_page(&p.owner, &p.w, Page::Workspace, "waiting", None, None)
        .await
        .unwrap();
    assert_eq!(
        waiting["items"][0]["replaces"]["text"],
        "Acme Traders pays 45 days after invoice."
    );
    assert_eq!(waiting["items"][0]["waiting_reason"], "disagreement");
    let fix = id_of(&db, "Acme Traders pays 60 days after invoice.").await;
    db.change_memory_note(&p.owner, &p.w, Page::Workspace, &fix, "keep")
        .await
        .unwrap();
    let archive = db
        .memory_page(&p.owner, &p.w, Page::Workspace, "archive", None, None)
        .await
        .unwrap();
    assert_eq!(
        texts(&archive, "items"),
        ["Acme Traders pays 45 days after invoice."]
    );

    // An admin writes, pins, ends and brings back a note; a member can't.
    let id = uuid::Uuid::new_v4().to_string();
    let note = |text: &str| changes::NoteInput {
        id: id.clone(),
        kind: "people".into(),
        topic: "Discounts".into(),
        text: text.into(),
        until: None,
    };
    assert!(
        db.save_memory_note(
            &p.meera,
            &p.w,
            Page::Workspace,
            &note("Anyone approves discounts.")
        )
        .await
        .is_err()
    );
    db.save_memory_note(
        &p.owner,
        &p.w,
        Page::Workspace,
        &note("Ravi approves discounts over 10%."),
    )
    .await
    .unwrap();
    db.save_memory_note(
        &p.owner,
        &p.w,
        Page::Workspace,
        &note("Ravi approves any discount over 10%."),
    )
    .await
    .unwrap();
    assert!(
        db.save_memory_note(
            &p.owner,
            &p.w,
            Page::Workspace,
            &changes::NoteInput {
                until: Some("2001-01-01".into()),
                ..note("x")
            }
        )
        .await
        .is_err()
    );
    db.change_memory_note(&p.owner, &p.w, Page::Workspace, &id, "pin")
        .await
        .unwrap();
    let essentials = db
        .memory_page(&p.owner, &p.w, Page::Workspace, "essentials", None, None)
        .await
        .unwrap();
    assert_eq!(
        essentials["items"][0]["text"],
        "Ravi approves any discount over 10%."
    );
    assert_eq!(essentials["items"][0]["pinned"], true);
    db.change_memory_note(&p.owner, &p.w, Page::Workspace, &id, "end")
        .await
        .unwrap();
    assert!(
        db.change_memory_note(&p.owner, &p.w, Page::Workspace, &id, "end")
            .await
            .is_err()
    );
    db.change_memory_note(&p.owner, &p.w, Page::Workspace, &id, "restore")
        .await
        .unwrap();
    let found = db
        .memory_page(
            &p.owner,
            &p.w,
            Page::Workspace,
            "notes",
            Some("discount"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        texts(&found, "items"),
        ["Ravi approves any discount over 10%."]
    );
    assert!(
        db.change_memory_note(&p.owner, &p.w, Page::Agent(&p.a), &id, "remove")
            .await
            .is_err(),
        "not on that page"
    );
    db.change_memory_note(&p.owner, &p.w, Page::Workspace, &id, "remove")
        .await
        .unwrap();
}
