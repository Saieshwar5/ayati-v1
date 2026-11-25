//! Schema 155: a new file with no folder named goes to the agent's own folder, in the
//! subfolder of the conversation it was made for, which only that conversation's
//! people, the agent's owners and admins open.
use super::super::tests::*;
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
    let mine = tool(
        &b,
        &job,
        "save_file",
        json!({"path": "report.txt", "description": "Quarterly sales figures for the board"}),
    )
    .await;
    assert_eq!(mine["state"], "saved", "{mine}");
    let mine = mine["id"].as_str().unwrap().to_owned();
    let report = db.library_get(&b.s.owner, &b.s.w, &mine).await.unwrap();
    // Its description is found by words the file itself doesn't hold.
    assert_eq!(
        report["description"],
        "Quarterly sales figures for the board"
    );
    let found = tool(
        &b,
        &job,
        "search_library",
        json!({"query": "quarterly board sales"}),
    )
    .await;
    assert_eq!(found["results"][0]["id"], json!(mine), "{found}");
    let described = crate::identity::postgres::Description {
        text: "  Board   pack, Q3 ".into(),
    };
    assert_eq!(
        db.library_describe(&b.s.owner, &b.s.w, &mine, &described)
            .await
            .unwrap()["description"],
        "Board pack, Q3"
    );
    assert!(
        db.library_describe(&b.s.member, &b.s.w, &mine, &described)
            .await
            .is_err()
    );
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
    // The agent's folder keeps what is in it for a year unless an admin changes that.
    assert_eq!(
        db.library_keeping(&b.s.owner, &b.s.w, &mine).await.unwrap()["folder_keep_days"],
        365
    );
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

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_file_attached_when_asking_is_kept_in_that_conversations_folder() {
    let b = bench().await;
    let db = &b.s.db;
    let room = chat(db, &b.s.member, &b.s.w, vec![], vec![b.s.agent.clone()]).await;
    let object = crate::identity::collaboration::Object {
        id: uuid::Uuid::new_v4().to_string(),
        name: "invoice.txt".into(),
        mime: "text/plain".into(),
        size: 13,
        checksum: checksum(b"Invoice 2041\n"),
    };
    db.save_object(&b.s.member, &b.s.w, &room, &object, false)
        .await
        .unwrap();
    // Chat stores the bytes; the Library item names them and stores nothing more.
    let key = format!("chat/{}/{}", b.s.w, object.id);
    db.library_files
        .put(&key, b"Invoice 2041\n".as_slice().into())
        .await
        .unwrap();
    let mut asking = message("What is this invoice?", Some(&b.s.agent));
    asking.attachments = vec![object.id.clone()];
    db.chat_send(&b.s.member, &b.s.w, &room, &asking)
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let kept = db
        .library_list(&b.s.member, &b.s.w, "shared", "", "", 0)
        .await
        .unwrap();
    let file = kept["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["title"] == "invoice.txt")
        .expect("kept in the Library")
        .clone();
    let id = file["id"].as_str().unwrap();
    // The agent opens it on its computer for this conversation, and for nobody else.
    let opened = tool(&b, &job, "open_document", json!({"id": id})).await;
    assert_eq!(opened["state"], "opened", "{opened}");
    assert_eq!(
        run(&b, &job, "cat library/invoice.txt").await,
        "Invoice 2041\n"
    );
    let outsider = user(db, "folder-outsider").await;
    join(db, &b.s.owner, &b.s.w, &outsider, "Member").await;
    db.chat_grant(&b.s.owner, &b.s.w, &outsider.id, &b.s.agent, true)
        .await
        .unwrap();
    let other = asked(&b, &outsider, "Anything new?").await;
    assert_eq!(
        tool(&b, &other, "open_document", json!({"id": id})).await["state"],
        "not_found",
        "another conversation"
    );
    assert_eq!(
        super::super::library::file_versions::storage_used(&db.db, &b.s.w)
            .await
            .unwrap(),
        0,
        "Chat's bytes, not the Library's"
    );
    // Deleting the Library item for good leaves Chat's file alone.
    let trash = ItemAction {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        action: "trash".into(),
        version: None,
    };
    db.library_action(&b.s.owner, &b.s.w, id, &trash)
        .await
        .unwrap();
    assert!(
        db.library_delete_forever(&b.s.owner, &b.s.w, id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(db.library_files.get(&key).await.is_ok());
}
