//! Library LB7: a folder's keeping rule deletes old items for good, never anything on
//! legal hold; deleted items can't be found, read or restored; only admins decide.
use super::super::{tests::*, *};
use super::folders::{ask, folder, page, setup};
use crate::identity::control::broker::invoke;
use crate::identity::library::model::ItemAction;
use crate::identity::postgres::{Hold, Keeping};
use serde_json::json;

fn act(action: &str, revision: i64) -> ItemAction {
    ItemAction {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        action: action.into(),
        version: None,
    }
}
async fn age(db: &PgStore, w: &str, id: &str, days: i64) {
    execute(
        &db.db,
        "UPDATE relay_library_items SET created=created-$3 WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into(), (days * 86_400).into()],
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn keeping_rules_delete_old_items_for_good_except_what_is_on_hold() {
    let s = setup().await;
    let db = &s.db;
    let policies = db.library_get(&s.owner, &s.w, &s.returns).await.unwrap()["folder"]
        .as_str()
        .unwrap()
        .to_owned();
    let keep = |days: Option<i32>| Keeping { keep_days: days };
    // Only admins set keeping rules, and only on folders.
    assert!(
        db.library_keeping_set(&s.member, &s.w, &policies, &keep(Some(30)))
            .await
            .is_err()
    );
    assert!(
        db.library_keeping_set(&s.owner, &s.w, &s.returns, &keep(Some(30)))
            .await
            .is_err()
    );
    assert!(
        db.library_keeping_set(&s.owner, &s.w, &policies, &keep(Some(0)))
            .await
            .is_err()
    );
    db.library_keeping_set(&s.owner, &s.w, &policies, &keep(Some(30)))
        .await
        .unwrap();
    let rule = db
        .library_keeping(&s.owner, &s.w, &s.returns)
        .await
        .unwrap();
    assert_eq!(rule["folder_keep_days"], 30);
    assert!(rule["deletes_at"].as_i64().unwrap() > now());
    // An old ID photo note and an old contract on hold, next to the new policy.
    let photo = page(
        "ID photo for Asha",
        "Passport photo, number X1234567.",
        Some(&policies),
    );
    db.library_save(&s.owner, &s.w, &photo).await.unwrap();
    let contract = page(
        "Supplier contract",
        "The supplier agreement runs to 2030.",
        Some(&policies),
    );
    db.library_save(&s.owner, &s.w, &contract).await.unwrap();
    age(db, &s.w, &photo.id, 40).await;
    age(db, &s.w, &contract.id, 40).await;
    let reason = |text: &str| Hold {
        reason: Some(text.into()),
    };
    assert!(
        db.library_hold_set(&s.member, &s.w, &contract.id, &reason("Dispute"))
            .await
            .is_err()
    );
    db.library_hold_set(&s.owner, &s.w, &contract.id, &reason("Supplier dispute"))
        .await
        .unwrap();
    assert_eq!(
        db.library_keeping(&s.member, &s.w, &s.returns)
            .await
            .is_ok(),
        false,
        "the member can't open Policies"
    );
    assert_eq!(
        db.library_keeping(&s.owner, &s.w, &contract.id)
            .await
            .unwrap()["hold"]["reason"],
        "Supplier dispute"
    );
    // The sweep deletes only the photo note: not the new policy, not the held contract.
    db.library_keeping_sweep(now()).await.unwrap();
    assert!(db.library_get(&s.owner, &s.w, &photo.id).await.is_err());
    db.library_get(&s.owner, &s.w, &contract.id).await.unwrap();
    db.library_get(&s.owner, &s.w, &s.returns).await.unwrap();
    let left: i64 = rows(&db.db, "SELECT (SELECT count(*) FROM relay_library_versions WHERE item=$1)+(SELECT count(*) FROM relay_library_passages WHERE item=$1) AS n", vec![photo.id.clone().into()]).await.unwrap()[0].try_get("", "n").unwrap();
    assert_eq!(left, 0, "versions and passages are gone");
    let tomb = rows(
        &db.db,
        "SELECT title,text,purged FROM relay_library_items WHERE id=$1",
        vec![photo.id.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(tomb[0].try_get::<String>("", "text").unwrap(), "");
    assert!(
        tomb[0]
            .try_get::<Option<i64>>("", "purged")
            .unwrap()
            .is_some()
    );
    let trash = db
        .library_list(&s.owner, &s.w, "trash", "", "", 0)
        .await
        .unwrap();
    assert!(
        trash["items"].as_array().unwrap().is_empty(),
        "not in Trash either"
    );
    let restore = db
        .library_action(&s.owner, &s.w, &photo.id, &act("restore", 2))
        .await
        .unwrap_err();
    assert!(
        restore.to_string().contains("deleted for good"),
        "{restore}"
    );
    // Agents can't find it.
    let conversation = chat(db, &s.owner, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(
        &s,
        &s.owner,
        &conversation,
        "What is Asha's passport number?",
    )
    .await;
    let found = invoke(
        db,
        &job,
        "search_library",
        json!({"query":"passport photo"}),
    )
    .await
    .unwrap();
    assert!(found["results"].as_array().unwrap().is_empty());
    db.chat_finish(&job, "Nothing.", "complete").await.unwrap();
    // On hold, the contract can't be trashed; lifted, it can.
    let refused = db
        .library_action(&s.owner, &s.w, &contract.id, &act("trash", 1))
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("legal hold"), "{refused}");
    db.library_hold_set(&s.owner, &s.w, &contract.id, &Hold { reason: None })
        .await
        .unwrap();
    db.library_action(&s.owner, &s.w, &contract.id, &act("trash", 1))
        .await
        .unwrap();
    // A hold on a folder covers what is in it.
    let hr_note = page("Old HR note", "Kept for the tribunal.", Some(&s.hr));
    db.library_save(&s.owner, &s.w, &hr_note).await.unwrap();
    age(db, &s.w, &hr_note.id, 400).await;
    db.library_keeping_set(&s.owner, &s.w, &s.hr, &keep(Some(365)))
        .await
        .unwrap();
    db.library_hold_set(&s.owner, &s.w, &s.hr, &reason("Tribunal"))
        .await
        .unwrap();
    db.library_keeping_sweep(now()).await.unwrap();
    db.library_get(&s.owner, &s.w, &hr_note.id).await.unwrap();
    assert!(
        db.library_action(&s.owner, &s.w, &s.salaries, &act("trash", 1))
            .await
            .is_err()
    );
    // Every rule, hold and deletion is audited.
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
        "Items in \"Policies\" are deleted 30 days after they are added",
        "Put \"Supplier contract\" on legal hold: Supplier dispute",
        "Lifted the legal hold on \"Supplier contract\"",
        "Deleted \"ID photo for Asha\" for good under the keeping rule of its folder",
    ] {
        assert!(said.contains(line), "missing {line}\n{said}");
    }
    let _ = folder;
}
