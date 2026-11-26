//! Schema 159: files belong to their conversation. An agent lists and opens what was
//! attached, attaches what it makes to its reply, and the Library gets a file only when
//! a person keeps it or the agent is told a folder.
use super::super::tests::*;
use super::workbench::{asked, bench, run, tool};
use crate::identity::library::model::*;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_file_the_agent_makes_is_attached_to_its_reply_for_the_people_in_the_conversation() {
    let b = bench().await;
    let db = &b.s.db;
    let job = asked(&b, &b.s.member, "Write me a report").await;
    run(&b, &job, "echo 'Member figures' > report.txt").await;
    let made = tool(&b, &job, "save_file", json!({"path": "report.txt"})).await;
    assert_eq!(made["state"], "attached", "{made}");
    // It is one of the conversation's files, marked as the agent's, and not in the Library.
    let files = db.chat_files(&b.s.member, &b.s.w, &job.chat).await.unwrap();
    assert_eq!(files["files"].as_array().unwrap().len(), 1);
    assert_eq!(files["files"][0]["name"], "report.txt");
    assert!(files["files"][0]["made_by"].is_string());
    let library = db
        .library_list(&b.s.member, &b.s.w, "all", "", "", 0)
        .await
        .unwrap();
    assert!(
        library["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["title"] != "report.txt")
    );
    let stored = db
        .files
        .chat_get(&b.s.w, made["file"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(&stored[..], b"Member figures\n");
    // Someone outside the conversation can't list it; on the agent's page they see nothing
    // of it, and the agent's keeper sees it marked as not their own conversation.
    let outsider = user(db, "files-outsider").await;
    join(db, &b.s.owner, &b.s.w, &outsider, "Member").await;
    db.chat_grant(&b.s.owner, &b.s.w, &outsider.id, &b.s.agent, true)
        .await
        .unwrap();
    assert!(db.chat_files(&outsider, &b.s.w, &job.chat).await.is_err());
    let theirs = db
        .agent_artifacts(&outsider, &b.s.w, &b.s.agent)
        .await
        .unwrap();
    assert!(theirs["files"].as_array().unwrap().is_empty());
    let mine = db
        .agent_artifacts(&b.s.member, &b.s.w, &b.s.agent)
        .await
        .unwrap();
    assert_eq!(
        (mine["files"][0]["mine"].as_bool(), mine["keeper"].as_bool()),
        (Some(true), Some(false))
    );
    let kept = db
        .agent_artifacts(&b.s.owner, &b.s.w, &b.s.agent)
        .await
        .unwrap();
    assert_eq!(
        (kept["files"][0]["mine"].as_bool(), kept["keeper"].as_bool()),
        (Some(false), Some(true))
    );
    // A reply carries at most eight files.
    for n in 0..7 {
        run(&b, &job, &format!("echo {n} > f{n}.txt")).await;
        assert_eq!(
            tool(&b, &job, "save_file", json!({"path": format!("f{n}.txt")})).await["state"],
            "attached"
        );
    }
    run(&b, &job, "echo 9 > ninth.txt").await;
    assert_eq!(
        tool(&b, &job, "save_file", json!({"path": "ninth.txt"})).await["state"],
        "invalid"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_attached_file_is_opened_by_the_agent_and_kept_in_the_library_by_a_person() {
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
    db.files
        .chat_put(&b.s.w, &object.id, b"Invoice 2041\n".as_slice().into())
        .await
        .unwrap();
    db.save_object(&b.s.member, &b.s.w, &room, &object, false)
        .await
        .unwrap();
    let mut asking = message("What is this invoice?", Some(&b.s.agent));
    asking.attachments = vec![object.id.clone()];
    db.chat_send(&b.s.member, &b.s.w, &room, &asking)
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    // The agent finds it among the conversation's files and opens it on its computer.
    let listed = tool(&b, &job, "conversation_files", json!({})).await;
    assert_eq!(listed["files"][0]["id"], json!(object.id), "{listed}");
    let opened = tool(&b, &job, "open_attachment", json!({"id": object.id})).await;
    assert_eq!(opened["path"], "attachments/invoice.txt", "{opened}");
    assert_eq!(
        run(&b, &job, "cat attachments/invoice.txt").await,
        "Invoice 2041\n"
    );
    // In another conversation the same agent can't open it.
    let other = asked(&b, &b.s.owner, "Anything new?").await;
    assert_eq!(
        tool(&b, &other, "open_attachment", json!({"id": object.id})).await["state"],
        "not_found"
    );
    // The person keeps it in the Library: a private copy, found by its words, once.
    let keep = crate::identity::postgres::conversation_files::KeepFile {
        id: uuid::Uuid::new_v4().to_string(),
        folder: None,
        title: None,
    };
    let kept = db
        .chat_file_keep(&b.s.member, &b.s.w, &object.id, &keep)
        .await
        .unwrap();
    assert_eq!(
        db.chat_file_keep(&b.s.member, &b.s.w, &object.id, &keep)
            .await
            .unwrap(),
        kept,
        "a retry is the same item"
    );
    let item = db.library_get(&b.s.member, &b.s.w, &keep.id).await.unwrap();
    assert_eq!(
        (
            item["title"].as_str(),
            item["visibility"].as_str(),
            item["kind"].as_str()
        ),
        (Some("invoice.txt"), Some("private"), Some("file"))
    );
    assert_eq!(
        db.library_list(&b.s.member, &b.s.w, "all", "2041", "", 0)
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        db.chat_files(&b.s.member, &b.s.w, &room).await.unwrap()["files"][0]["library_item"],
        json!(keep.id)
    );
    // Chat's own rule still decides who opens a conversation's file: not an admin outside it.
    assert!(
        db.chat_file_keep(&b.s.owner, &b.s.w, &object.id, &keep)
            .await
            .is_err()
    );
    // Its description is searched with its text.
    let described = crate::identity::postgres::Description {
        text: "  Acme   supplier bill ".into(),
    };
    assert_eq!(
        db.library_describe(&b.s.member, &b.s.w, &keep.id, &described)
            .await
            .unwrap()["description"],
        "Acme supplier bill"
    );
    assert_eq!(
        db.library_list(&b.s.member, &b.s.w, "all", "", "", 0)
            .await
            .unwrap()["items"][0]["description"],
        "Acme supplier bill"
    );
}
