//! The nightly tidy-up (schema 142): notes past their date end, copies merge,
//! unused notes fade into the archive and are deleted after a year, old notes
//! fold into a topic's summary; every change can be undone.
use super::super::tests::*;
use super::manage::Page;
use super::*;

const DAY: i64 = 86_400_000;

/// A workspace note as the tidy-up finds it.
async fn note(db: &PgStore, w: &str, text: &str, kind: &str, fields: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    execute(
        &db.db,
        "INSERT INTO relay_memories(workspace,id,scope,kind,topic,text,source,strength) VALUES($1,$2,'workspace',$3,'',$4,'agent',50)",
        vec![w.into(), id.clone().into(), kind.into(), text.into()],
    )
    .await
    .unwrap();
    execute(
        &db.db,
        &format!("UPDATE relay_memories SET {fields} WHERE workspace=$1 AND id=$2"),
        vec![w.into(), id.clone().into()],
    )
    .await
    .unwrap();
    id
}
async fn state(db: &PgStore, id: &str) -> Option<String> {
    query(
        &db.db,
        "SELECT state FROM relay_memories WHERE id=$1",
        vec![id.into()],
    )
    .await
    .unwrap()
    .map(|r| r.try_get("", "state").unwrap())
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_tidy_up_ends_merges_fades_deletes_and_each_change_can_be_undone() {
    let db = fixture().await;
    let owner = user(&db, "tidy-owner").await;
    let w = workspace(&db, &owner).await;
    let now = now() * 1000;
    let set = |sql: String| sql;
    let leave = note(
        &db,
        &w,
        "Priya is on leave this week.",
        "status",
        &set(format!("until_ms={}", now - DAY)),
    )
    .await;
    let first = note(
        &db,
        &w,
        "Acme pays 45 days after invoice.",
        "customer",
        &set(format!("created_ms={}", now - 10 * DAY)),
    )
    .await;
    let copy = note(
        &db,
        &w,
        "acme pays 45 days after invoice",
        "customer",
        &set(format!("used=3,created_ms={}", now - DAY)),
    )
    .await;
    let unused = note(
        &db,
        &w,
        "Stock count is on Friday.",
        "status",
        &set(format!("used_ms={}", now - 120 * DAY)),
    )
    .await;
    let pinned = note(
        &db,
        &w,
        "Quotes are always sent as PDF.",
        "status",
        &set(format!("pinned=true,used_ms={}", now - 400 * DAY)),
    )
    .await;
    let old = note(
        &db,
        &w,
        "An old archived note.",
        "term",
        &set(format!("state='archived',changed_ms={}", now - 400 * DAY)),
    )
    .await;

    assert_eq!(
        db.memory_tidy_due(now, 10).await.unwrap(),
        std::slice::from_ref(&w)
    );
    let done = db.tidy_memory(&w, now).await.unwrap();
    assert_eq!(
        done,
        serde_json::json!({"expired":1,"merged":1,"archived":1,"deleted":1})
    );
    assert_eq!(state(&db, &leave).await.as_deref(), Some("ended"));
    assert_eq!(state(&db, &first).await.as_deref(), Some("active"));
    assert_eq!(state(&db, &copy).await.as_deref(), Some("ended"));
    assert_eq!(state(&db, &unused).await.as_deref(), Some("archived"));
    assert_eq!(
        state(&db, &pinned).await.as_deref(),
        Some("active"),
        "a pinned note never fades"
    );
    assert_eq!(state(&db, &old).await, None);
    let merged = query(
        &db.db,
        "SELECT strength,used FROM relay_memories WHERE id=$1",
        vec![first.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(merged.try_get::<i32>("", "strength").unwrap(), 65);
    assert_eq!(merged.try_get::<i32>("", "used").unwrap(), 3);
    assert!(
        db.memory_tidy_due(now, 10).await.unwrap().is_empty(),
        "once a day"
    );
    assert_eq!(
        db.memory_tidy_due(now + DAY, 10).await.unwrap(),
        std::slice::from_ref(&w)
    );

    // Every change is listed for the admin and can be undone once.
    let changes = db
        .memory_page(&owner, &w, Page::Workspace, "changes", None, None)
        .await
        .unwrap();
    let items = changes["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    let merge = items.iter().find(|c| c["action"] == "merged").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    db.undo_memory_change(&owner, &w, Page::Workspace, &merge)
        .await
        .unwrap();
    assert_eq!(state(&db, &copy).await.as_deref(), Some("active"));
    let back = query(
        &db.db,
        "SELECT strength FROM relay_memories WHERE id=$1",
        vec![first.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(back.try_get::<i32>("", "strength").unwrap(), 50);
    assert!(
        db.undo_memory_change(&owner, &w, Page::Workspace, &merge)
            .await
            .is_err()
    );
    let deleted = items.iter().find(|c| c["action"] == "deleted").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    db.undo_memory_change(&owner, &w, Page::Workspace, &deleted)
        .await
        .unwrap();
    assert_eq!(
        state(&db, &old).await.as_deref(),
        Some("archived"),
        "a deleted note comes back"
    );

    // Old notes on a topic fold into its summary; undone, they come back.
    let mut folded = vec![];
    for text in [
        "Acme orders through Priya.",
        "Acme wants quotes as PDF.",
        "Acme ships from Pune.",
    ] {
        folded.push(
            note(
                &db,
                &w,
                text,
                "customer",
                &set(format!(
                    "topic='Acme Traders',created_ms={}",
                    now - 100 * DAY
                )),
            )
            .await,
        );
    }
    note(
        &db,
        &w,
        "Acme's owner is copied on price changes.",
        "customer",
        "topic='Acme Traders',pinned=true",
    )
    .await;
    let work = db.memory_topics_to_summarise(&w, now, 5).await.unwrap();
    let acme = work.iter().find(|t| t.topic == "Acme Traders").unwrap();
    assert_eq!(acme.notes.len(), 4);
    assert_eq!(acme.fold.len(), 3, "a pinned note stays a note");
    db.save_memory_summary(
        acme,
        "Acme Traders orders through Priya, wants quotes as PDF and ships from Pune.",
        now,
    )
    .await
    .unwrap();
    for id in &folded {
        assert_eq!(state(&db, id).await.as_deref(), Some("folded"));
    }
    let topics = db
        .memory_page(&owner, &w, Page::Workspace, "topics", None, None)
        .await
        .unwrap();
    assert_eq!(
        topics["items"][0]["summary"],
        "Acme Traders orders through Priya, wants quotes as PDF and ships from Pune."
    );
    let changes = db
        .memory_page(&owner, &w, Page::Workspace, "changes", None, None)
        .await
        .unwrap();
    let summary = changes["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["action"] == "summarised")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    db.undo_memory_change(&owner, &w, Page::Workspace, &summary)
        .await
        .unwrap();
    for id in &folded {
        assert_eq!(state(&db, id).await.as_deref(), Some("active"));
    }
    let topics = db
        .memory_page(&owner, &w, Page::Workspace, "topics", None, None)
        .await
        .unwrap();
    assert_eq!(topics["items"][0]["summary"], Value::Null);

    // An admin writes a topic's summary by hand; a member can't.
    let input = |summary: &str| changes::TopicInput {
        topic: "Acme Traders".into(),
        summary: summary.into(),
    };
    assert!(
        db.save_memory_topic(
            &user(&db, "tidy-stranger").await,
            &w,
            Page::Workspace,
            &input("x")
        )
        .await
        .is_err()
    );
    db.save_memory_topic(&owner, &w, Page::Workspace, &input("Acme pays in 45 days."))
        .await
        .unwrap();
    let topics = db
        .memory_page(&owner, &w, Page::Workspace, "topics", None, None)
        .await
        .unwrap();
    assert_eq!(topics["items"][0]["summary"], "Acme pays in 45 days.");
}
