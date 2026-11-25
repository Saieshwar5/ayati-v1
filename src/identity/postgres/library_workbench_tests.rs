//! Schema 154: an agent opens a Library item on its computer and saves it back as the
//! next version, only where its level, the person it works for and the audience allow,
//! and never over a newer revision.
use super::super::{tests::*, *};
use super::folders::{Setup, ask, grant, page, setup, share};
use crate::identity::chat::model::Job;
use crate::identity::computers::{Computers, fake};
use crate::identity::control::broker::invoke;
use crate::identity::library::model::*;
use serde_json::json;

struct Bench {
    s: Setup,
    _fake: fake::Fake,
    _dir: tempfile::TempDir,
    policies: String,
}
/// The Library setup with file storage, and the agent's computer switched on.
async fn bench() -> Bench {
    let mut s = setup().await;
    let fake = fake::start().await;
    let dir = tempfile::tempdir().unwrap();
    s.db.computers = Some(Computers::new(fake.providers_file(dir.path())));
    s.db.library_files = crate::identity::library::Storage::Local(dir.path().join("objects"));
    execute(
        &s.db.db,
        "UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{computer}',$2) WHERE id=$1",
        vec![
            s.agent.clone().into(),
            json!({"on": true, "size": "small"}).into(),
        ],
    )
    .await
    .unwrap();
    let policies = s.db.library_get(&s.owner, &s.w, &s.returns).await.unwrap()["folder"]
        .as_str()
        .unwrap()
        .to_owned();
    Bench {
        s,
        _fake: fake,
        _dir: dir,
        policies,
    }
}
/// Gives the agent a level on Policies, for all staff.
async fn level(b: &Bench, level: &str) {
    let revision =
        b.s.db
            .library_get(&b.s.owner, &b.s.w, &b.policies)
            .await
            .unwrap()["access_revision"]
            .as_i64()
            .unwrap();
    let mut g = grant(&b.s.agent, "staff");
    g.level = level.into();
    b.s.db
        .library_share(
            &b.s.owner,
            &b.s.w,
            &b.policies,
            &share(revision, "private", vec![], vec![g]),
        )
        .await
        .unwrap();
}
/// A new request; the one before it has finished and let go of the computer.
async fn asked(b: &Bench, person: &User, text: &str) -> Job {
    execute(
        &b.s.db.db,
        "UPDATE relay_computers SET held_by=NULL,held_until=0 WHERE workspace=$1",
        vec![b.s.w.clone().into()],
    )
    .await
    .unwrap();
    let room = chat(&b.s.db, person, &b.s.w, vec![], vec![b.s.agent.clone()]).await;
    ask(&b.s, person, &room, text).await
}
async fn tool(b: &Bench, job: &Job, name: &str, input: Value) -> Value {
    invoke(&b.s.db, job, name, input).await.unwrap()
}
async fn run(b: &Bench, job: &Job, command: &str) -> String {
    let ran = tool(b, job, "computer_run", json!({"command": command})).await;
    assert_eq!(ran["exit_code"], 0, "{ran}");
    ran["stdout"].as_str().unwrap_or("").to_owned()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_page_is_opened_as_markdown_and_saved_back_as_its_next_version() {
    let b = bench().await;
    let db = &b.s.db;
    level(&b, "edit").await;
    let job = asked(&b, &b.s.owner, "Make the refund window 45 days").await;
    let opened = tool(&b, &job, "open_document", json!({"id": b.s.returns})).await;
    assert_eq!(
        (
            opened["state"].as_str(),
            opened["path"].as_str(),
            opened["revision"].as_i64()
        ),
        (Some("opened"), Some("library/Returns policy.md"), Some(1))
    );
    assert_eq!(
        run(&b, &job, "cat 'library/Returns policy.md'").await,
        "The refund window is 30 days from delivery.\n"
    );
    run(&b, &job, "printf '# Returns\\n\\nThe refund window is **45 days** from delivery.\\n' > 'library/Returns policy.md'").await;
    let saved = tool(
        &b,
        &job,
        "save_file",
        json!({"path": "library/Returns policy.md"}),
    )
    .await;
    assert_eq!(
        (saved["state"].as_str(), saved["revision"].as_i64()),
        (Some("edited"), Some(2))
    );
    let page = db
        .library_get(&b.s.owner, &b.s.w, &b.s.returns)
        .await
        .unwrap();
    assert_eq!(page["document"]["content"][0]["type"], "heading");
    assert_eq!(
        page["document"]["content"][1]["content"][1]["marks"][0]["type"],
        "bold"
    );
    let history = db
        .library_versions(&b.s.owner, &b.s.w, &b.s.returns)
        .await
        .unwrap();
    assert!(
        history["versions"][0]["agent"].is_string(),
        "marked with the agent"
    );
    // Saving the same file again continues from the version just written.
    run(
        &b,
        &job,
        "printf 'Now 60 days.\\n' >> 'library/Returns policy.md'",
    )
    .await;
    let again = tool(
        &b,
        &job,
        "save_file",
        json!({"path": "library/Returns policy.md"}),
    )
    .await;
    assert_eq!(again["revision"], 3, "{again}");
    // A person's newer edit is not overwritten: the agent opens it again.
    let mut theirs = page_from(&page, 3);
    theirs.request = uuid::Uuid::new_v4().to_string();
    db.library_save(&b.s.owner, &b.s.w, &theirs).await.unwrap();
    let stale = tool(
        &b,
        &job,
        "save_file",
        json!({"path": "library/Returns policy.md"}),
    )
    .await;
    assert_eq!(
        (stale["state"].as_str(), stale["revision"].as_i64()),
        (Some("changed"), Some(4))
    );
    // The answer cites what was opened, and a file from nowhere needs a folder.
    let cited: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_library_reads WHERE reply=$1 AND item=$2 AND cited",
        vec![job.reply.clone().into(), b.s.returns.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(cited, 1);
    assert_eq!(
        tool(
            &b,
            &job,
            "save_file",
            json!({"path": "missing.txt", "folder": b.policies})
        )
        .await["state"],
        "invalid"
    );
}
/// The page as a person would save it again from `revision`.
fn page_from(current: &Value, revision: i64) -> SavePage {
    let mut next = page("Returns policy", "People edited this meanwhile.", None);
    next.id = current["id"].as_str().unwrap().to_owned();
    next.revision = revision;
    next
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn saving_follows_the_agents_level_the_person_and_the_audience() {
    let b = bench().await;
    let db = &b.s.db;
    let path = json!({"path": "library/Returns policy.md"});
    // Read: it opens, and the change stays on the computer.
    let job = asked(&b, &b.s.owner, "Tidy the policy").await;
    tool(&b, &job, "open_document", json!({"id": b.s.returns})).await;
    run(
        &b,
        &job,
        "echo 'Extra line.' >> 'library/Returns policy.md'",
    )
    .await;
    assert_eq!(
        tool(&b, &job, "save_file", path.clone()).await["state"],
        "not_allowed"
    );
    // Suggest: the save waits for a person.
    level(&b, "suggest").await;
    let job = asked(&b, &b.s.owner, "Tidy the policy again").await;
    tool(&b, &job, "open_document", json!({"id": b.s.returns})).await;
    run(
        &b,
        &job,
        "echo 'Extra line.' >> 'library/Returns policy.md'",
    )
    .await;
    let waiting = tool(
        &b,
        &job,
        "save_file",
        json!({"path": "library/Returns policy.md", "note": "Clearer"}),
    )
    .await;
    assert_eq!(waiting["state"], "suggested", "{waiting}");
    assert_eq!(
        db.library_get(&b.s.owner, &b.s.w, &b.s.returns)
            .await
            .unwrap()["revision"],
        1
    );
    // Edit, for a colleague who can't edit the page: they can't have it changed.
    level(&b, "edit").await;
    let job = asked(&b, &b.s.member, "Change it for me").await;
    assert_eq!(
        tool(&b, &job, "open_document", json!({"id": b.s.returns})).await["state"],
        "opened"
    );
    assert_eq!(
        tool(&b, &job, "save_file", path).await["state"],
        "not_allowed"
    );
    // What the owner's requests opened is gone from the computer by now.
    assert_eq!(
        run(&b, &job, "find .. -path '*/library/*' | wc -l")
            .await
            .trim(),
        "1",
        "only this request's own copy"
    );
    // HR is for people who can see it: the colleague's request can't open it at all.
    assert_eq!(
        tool(&b, &job, "open_document", json!({"id": b.s.salaries})).await["state"],
        "not_found"
    );
    assert_eq!(
        tool(&b, &job, "open_document", json!({"id": b.s.hr})).await["state"],
        "not_found",
        "a folder isn't a file"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn files_and_tables_are_worked_on_and_new_files_saved_in_a_folder() {
    let b = bench().await;
    let db = &b.s.db;
    // A stored file in Policies.
    let bytes = b"Desk: 1200\n";
    let upload = UploadFile {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        title: "prices.txt".into(),
        size: bytes.len() as i64,
        checksum: checksum(bytes),
        replaces: None,
    };
    db.library_upload_start(&b.s.owner, &b.s.w, &upload)
        .await
        .unwrap();
    db.library_files
        .put(
            &format!("library/{}/{}", b.s.w, upload.id),
            bytes.as_slice().into(),
        )
        .await
        .unwrap();
    db.library_upload_finish(
        &b.s.owner,
        &b.s.w,
        &upload.id,
        "text/plain",
        "Desk: 1200",
        "ready",
    )
    .await
    .unwrap();
    db.library_move(
        &b.s.owner,
        &b.s.w,
        &upload.id,
        &MoveItem {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            folder: Some(b.policies.clone()),
        },
    )
    .await
    .unwrap();
    level(&b, "edit").await;
    let job = asked(&b, &b.s.owner, "Raise the desk price").await;
    let opened = tool(&b, &job, "open_document", json!({"id": upload.id})).await;
    assert_eq!(opened["path"], "library/prices.txt", "{opened}");
    assert_eq!(
        run(&b, &job, "cat library/prices.txt").await,
        "Desk: 1200\n"
    );
    run(&b, &job, "echo 'Desk: 1250' > library/prices.txt").await;
    let saved = tool(&b, &job, "save_file", json!({"path": "library/prices.txt"})).await;
    assert_eq!(
        (saved["state"].as_str(), saved["revision"].as_i64()),
        (Some("saved"), Some(2)),
        "{saved}"
    );
    let file = db
        .library_file(&b.s.owner, &b.s.w, &upload.id)
        .await
        .unwrap();
    assert_eq!(file["text"], "Desk: 1250\n");
    assert_eq!(
        &db.library_files
            .get(file["object_key"].as_str().unwrap())
            .await
            .unwrap()[..],
        b"Desk: 1250\n"
    );
    let history = db
        .library_versions(&b.s.owner, &b.s.w, &upload.id)
        .await
        .unwrap();
    assert_eq!(history["versions"].as_array().unwrap().len(), 2);
    assert!(history["versions"][0]["agent"].is_string());
    // A table is a CSV file; a number column refuses text and nothing changes.
    let table = json!({"type":"table","columns":[{"name":"Product","type":"text"},{"name":"Price","type":"number"}],"rows":[["Desk",1200]]});
    let prices = crate::identity::postgres::SaveTable {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "Price list".into(),
        table,
        folder: Some(b.policies.clone()),
    };
    db.library_table_save(&b.s.owner, &b.s.w, &prices)
        .await
        .unwrap();
    let opened = tool(&b, &job, "open_document", json!({"id": prices.id})).await;
    assert_eq!(opened["path"], "library/Price list.csv");
    assert_eq!(
        run(&b, &job, "cat 'library/Price list.csv'").await,
        "Product,Price\nDesk,1200\n"
    );
    run(
        &b,
        &job,
        "printf 'Product,Price\\nDesk,cheap\\n' > 'library/Price list.csv'",
    )
    .await;
    assert_eq!(
        tool(
            &b,
            &job,
            "save_file",
            json!({"path": "library/Price list.csv"})
        )
        .await["state"],
        "invalid"
    );
    run(
        &b,
        &job,
        "printf 'Product,Price\\nDesk,1250\\nChair,300\\n' > 'library/Price list.csv'",
    )
    .await;
    let saved = tool(
        &b,
        &job,
        "save_file",
        json!({"path": "library/Price list.csv"}),
    )
    .await;
    assert_eq!(
        (saved["state"].as_str(), saved["revision"].as_i64()),
        (Some("saved"), Some(2)),
        "{saved}"
    );
    assert_eq!(
        db.library_get(&b.s.owner, &b.s.w, &prices.id)
            .await
            .unwrap()["document"]["rows"],
        json!([["Desk", 1250], ["Chair", 300]])
    );
    // A new file needs "save here" on the folder; then it is filed there for the person.
    run(&b, &job, "echo 'May report' > report.txt").await;
    let new = json!({"path": "report.txt", "folder": b.policies, "title": "May report.txt"});
    assert_eq!(
        tool(&b, &job, "save_file", new.clone()).await["state"],
        "not_allowed"
    );
    level(&b, "save").await;
    let job = asked(&b, &b.s.owner, "Save the report").await;
    run(&b, &job, "echo 'May report' > report.txt").await;
    let made = tool(&b, &job, "save_file", new).await;
    assert_eq!(made["state"], "saved", "{made}");
    let report = db
        .library_get(&b.s.owner, &b.s.w, made["id"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(
        (
            report["title"].as_str(),
            report["folder"].as_str(),
            report["kind"].as_str()
        ),
        (
            Some("May report.txt"),
            Some(b.policies.as_str()),
            Some("file")
        )
    );
    let said: Vec<String> = rows(&db.db, "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='library_file_saved_by_agent' ORDER BY id", vec![b.s.w.clone().into()]).await.unwrap().into_iter().map(|r| r.try_get("", "summary").unwrap()).collect();
    assert_eq!(said.len(), 3, "{said:?}");
    assert!(
        said[0].ends_with("saved a new version of \"prices.txt\" (revision 2)"),
        "{said:?}"
    );
    assert!(
        said[2].ends_with("saved \"May report.txt\" in \"Policies\""),
        "{said:?}"
    );
}

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
