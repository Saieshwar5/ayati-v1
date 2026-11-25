//! Schema 155: a new file with no folder named goes to the agent's own folder, in the
//! subfolder of the conversation it was made for, which only that conversation's
//! people, the agent's owners and admins open.
use super::folders::share;
use super::workbench::{asked, bench, run, tool};
use crate::identity::library::model::*;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_new_file_with_no_folder_goes_to_the_agents_folder_for_that_conversation() {
    let b = bench().await;
    let db = &b.s.db;
    // The owner's request: the report lands in the agent's folder for their conversation.
    let job = asked(&b, &b.s.owner, "Write me a report").await;
    run(&b, &job, "echo 'Owner figures' > report.txt").await;
    let mine = tool(&b, &job, "save_file", json!({"path": "report.txt"})).await;
    assert_eq!(mine["state"], "saved", "{mine}");
    let mine = mine["id"].as_str().unwrap().to_owned();
    let report = db.library_get(&b.s.owner, &b.s.w, &mine).await.unwrap();
    assert_eq!(
        report["granted"], true,
        "theirs because they are in the conversation"
    );
    let place = report["folder"].as_str().unwrap().to_owned();
    assert_eq!(
        report["source"],
        json!(place),
        "the conversation's own sharing"
    );
    // A second file from the same conversation goes to the same subfolder.
    run(&b, &job, "echo 'More' > more.txt").await;
    let more = tool(&b, &job, "save_file", json!({"path": "more.txt"})).await;
    assert_eq!(
        db.library_get(&b.s.owner, &b.s.w, more["id"].as_str().unwrap())
            .await
            .unwrap()["folder"],
        json!(place)
    );
    // A colleague asking the same agent gets a subfolder of their own and never the owner's file.
    let job = asked(&b, &b.s.member, "Write me one too").await;
    assert!(db.library_get(&b.s.member, &b.s.w, &mine).await.is_err());
    assert_eq!(
        tool(&b, &job, "open_document", json!({"id": mine})).await["state"],
        "not_found"
    );
    run(&b, &job, "echo 'Member figures' > report.txt").await;
    let theirs = tool(&b, &job, "save_file", json!({"path": "report.txt"})).await;
    assert_eq!(theirs["state"], "saved", "{theirs}");
    let theirs = db
        .library_get(&b.s.member, &b.s.w, theirs["id"].as_str().unwrap())
        .await
        .unwrap();
    assert_ne!(theirs["folder"], json!(place));
    assert_eq!(theirs["editable"], true);
    // Both subfolders sit in the agent's one folder, which only its keepers open.
    let top = db.library_get(&b.s.owner, &b.s.w, &place).await.unwrap()["folder"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        db.library_get(&b.s.owner, &b.s.w, theirs["folder"].as_str().unwrap())
            .await
            .unwrap()["folder"],
        json!(top)
    );
    assert!(db.library_get(&b.s.member, &b.s.w, &top).await.is_err());
    assert_eq!(
        db.library_list(&b.s.owner, &b.s.w, "everything", "", &top, 0)
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // Nobody shares, moves or trashes an agent's folder by hand.
    let folder = db.library_get(&b.s.owner, &b.s.w, &place).await.unwrap();
    let revision = folder["access_revision"].as_i64().unwrap();
    assert!(
        db.library_share(
            &b.s.owner,
            &b.s.w,
            &place,
            &share(revision, "workspace", vec![], vec![])
        )
        .await
        .is_err()
    );
    assert!(
        db.library_move(
            &b.s.owner,
            &b.s.w,
            &place,
            &MoveItem {
                request: uuid::Uuid::new_v4().to_string(),
                revision,
                folder: None
            }
        )
        .await
        .is_err()
    );
    let trash = ItemAction {
        request: uuid::Uuid::new_v4().to_string(),
        revision: folder["revision"].as_i64().unwrap(),
        action: "trash".into(),
        version: None,
    };
    assert!(
        db.library_action(&b.s.owner, &b.s.w, &top, &trash)
            .await
            .is_err()
    );
}
