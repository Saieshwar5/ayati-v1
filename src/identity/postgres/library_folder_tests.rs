//! Library LB1: folders share their access with their items, and "use for" decides
//! whose questions an agent may answer from a document.
use super::super::{tests::*, *};
use crate::identity::control::broker::invoke;
use crate::identity::library::model::*;
use serde_json::json;

pub(super) fn page(title: &str, text: &str, folder: Option<&str>) -> SavePage {
    SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: title.into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":text}]}]}),
        folder: folder.map(str::to_owned),
    }
}
pub(super) async fn folder(db: &PgStore, user: &User, w: &str, title: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    db.library_folder_create(
        user,
        w,
        &CreateFolder {
            id: id.clone(),
            request: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
        },
    )
    .await
    .unwrap();
    id
}
pub(super) fn share(
    revision: i64,
    visibility: &str,
    people: Vec<(&User, &str)>,
    agents: Vec<AgentGrant>,
) -> ShareItem {
    ShareItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        visibility: visibility.into(),
        people: people
            .into_iter()
            .map(|(u, a)| PersonGrant {
                user: u.id.clone(),
                access: a.into(),
            })
            .collect(),
        agents,
    }
}
pub(super) fn grant(agent: &str, use_for: &str) -> AgentGrant {
    AgentGrant {
        agent: agent.into(),
        level: "read".into(),
        use_for: use_for.into(),
    }
}
fn moving(revision: i64, folder: Option<&str>) -> MoveItem {
    MoveItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        folder: folder.map(str::to_owned),
    }
}
fn titles(listing: &Value) -> Vec<String> {
    listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn items_in_a_folder_follow_its_sharing() {
    let db = fixture().await;
    let owner = user(&db, "folder-owner").await;
    let member = user(&db, "folder-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let sales = folder(&db, &owner, &w, "Sales").await;
    let quote = page("Quote template", "Price per unit", Some(&sales));
    db.library_save(&owner, &w, &quote).await.unwrap();
    assert!(db.library_get(&member, &w, &quote.id).await.is_err());
    // All shows one level, folders first; the folder lists its own items.
    let top = db.library_list(&owner, &w, "all", "", "", 0).await.unwrap();
    assert_eq!(titles(&top), ["Sales"]);
    assert_eq!(top["items"][0]["item_count"], 1);
    let inside = db
        .library_list(&owner, &w, "all", "", &sales, 0)
        .await
        .unwrap();
    assert_eq!(titles(&inside), ["Quote template"]);
    assert_eq!(inside["items"][0]["folder_title"], "Sales");
    // Sharing the folder shares what is in it; the item itself can't be shared apart.
    db.library_share(&owner, &w, &sales, &share(1, "workspace", vec![], vec![]))
        .await
        .unwrap();
    let seen = db.library_get(&member, &w, &quote.id).await.unwrap();
    assert_eq!(
        (seen["visibility"].as_str(), seen["editable"].as_bool()),
        (Some("workspace"), Some(false))
    );
    assert_eq!(
        seen["access_revision"], 2,
        "the folder's change fences the item too"
    );
    assert!(
        db.library_share(
            &owner,
            &w,
            &quote.id,
            &share(2, "workspace", vec![], vec![])
        )
        .await
        .is_err()
    );
    // Shared lists items wherever they are filed (same-second updates order by id).
    let mut shared = titles(
        &db.library_list(&member, &w, "shared", "", "", 0)
            .await
            .unwrap(),
    );
    shared.sort();
    assert_eq!(shared, ["Quote template", "Sales"]);
    // Only people who can edit a folder add to it.
    assert!(
        db.library_save(&member, &w, &page("Member quote", "x", Some(&sales)))
            .await
            .is_err()
    );
    let mine = page("Member notes", "Discount ideas", None);
    db.library_save(&member, &w, &mine).await.unwrap();
    assert!(
        db.library_move(&member, &w, &mine.id, &moving(1, Some(&sales)))
            .await
            .is_err()
    );
    db.library_share(
        &owner,
        &w,
        &sales,
        &share(1 + 1, "workspace", vec![(&member, "edit")], vec![]),
    )
    .await
    .unwrap();
    db.library_move(&member, &w, &mine.id, &moving(1, Some(&sales)))
        .await
        .unwrap();
    assert_eq!(
        db.library_get(&owner, &w, &mine.id).await.unwrap()["folder"],
        sales.as_str()
    );
    assert!(
        db.library_get(&member, &w, &quote.id).await.unwrap()["editable"] == true,
        "edit on the folder edits its pages"
    );
    // Out of the folder, an item is private to its creator again.
    db.library_move(&member, &w, &mine.id, &moving(2, None))
        .await
        .unwrap();
    assert!(db.library_get(&owner, &w, &mine.id).await.is_err());
    // A folder goes to Trash only when empty; an item restored from it comes back unfiled.
    let action = |action: &str, revision: i64| ItemAction {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        action: action.into(),
        version: None,
    };
    let revision = |v: Value| v["revision"].as_i64().unwrap();
    let r = revision(db.library_get(&owner, &w, &sales).await.unwrap());
    assert!(
        db.library_action(&owner, &w, &sales, &action("trash", r))
            .await
            .is_err()
    );
    let q = revision(db.library_get(&owner, &w, &quote.id).await.unwrap());
    db.library_action(&owner, &w, &quote.id, &action("trash", q))
        .await
        .unwrap();
    db.library_action(&owner, &w, &sales, &action("trash", r))
        .await
        .unwrap();
    db.library_action(&owner, &w, &quote.id, &action("restore", q + 1))
        .await
        .unwrap();
    let restored = db.library_get(&owner, &w, &quote.id).await.unwrap();
    assert_eq!(
        (
            restored["folder"].is_null(),
            restored["visibility"].as_str()
        ),
        (true, Some("private"))
    );
    assert!(db.library_get(&member, &w, &quote.id).await.is_err());
    let audit: i64 = query(&db.db, "SELECT count(*) AS n FROM relay_audit WHERE workspace=$1 AND action IN ('library_folder_created','library_moved')", vec![w.clone().into()])
        .await.unwrap().unwrap().try_get("", "n").unwrap();
    assert_eq!(audit, 3);
}

pub(super) struct Setup {
    pub(super) db: PgStore,
    pub(super) owner: User,
    pub(super) member: User,
    pub(super) w: String,
    pub(super) agent: String,
    pub(super) returns: String,
    pub(super) salaries: String,
    pub(super) hr: String,
}
pub(super) async fn setup() -> Setup {
    let db = fixture().await;
    let owner = user(&db, "use-owner").await;
    let member = user(&db, "use-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let (_, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &member.id, &agent, true)
        .await
        .unwrap();
    let policies = folder(&db, &owner, &w, "Policies").await;
    let returns = page(
        "Returns policy",
        "The refund window is 30 days from delivery.",
        Some(&policies),
    );
    db.library_save(&owner, &w, &returns).await.unwrap();
    db.library_share(
        &owner,
        &w,
        &policies,
        &share(1, "private", vec![], vec![grant(&agent, "staff")]),
    )
    .await
    .unwrap();
    let hr = folder(&db, &owner, &w, "HR").await;
    let salaries = page("Salaries", "Salary band for support is 40k.", Some(&hr));
    db.library_save(&owner, &w, &salaries).await.unwrap();
    db.library_share(
        &owner,
        &w,
        &hr,
        &share(1, "private", vec![], vec![grant(&agent, "viewers")]),
    )
    .await
    .unwrap();
    Setup {
        db,
        owner,
        member,
        w,
        agent,
        returns: returns.id,
        salaries: salaries.id,
        hr,
    }
}
pub(super) async fn ask(
    s: &Setup,
    person: &User,
    chat: &str,
    text: &str,
) -> crate::identity::chat::model::Job {
    s.db.chat_send(person, &s.w, chat, &message(text, Some(&s.agent)))
        .await
        .unwrap();
    s.db.claim_job().await.unwrap().unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn use_for_decides_whose_questions_a_document_answers() {
    let s = setup().await;
    let db = &s.db;
    // A colleague without access: the staff policy answers, the HR folder never does.
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(&s, &s.member, &theirs, "What is our refund window?").await;
    let found = invoke(db, &job, "search_library", json!({"query":"refund window"}))
        .await
        .unwrap();
    assert_eq!(found["results"][0]["title"], "Returns policy");
    assert!(
        found["results"][0]["preview"]
            .as_str()
            .unwrap()
            .contains("30 days")
    );
    let salary = invoke(db, &job, "search_library", json!({"query":"salary band"}))
        .await
        .unwrap();
    assert!(salary["results"].as_array().unwrap().is_empty());
    let hidden = invoke(db, &job, "read_document", json!({"id":s.salaries}))
        .await
        .unwrap();
    assert_eq!(hidden["state"], "not_found");
    let listed = invoke(db, &job, "list_folder", json!({})).await.unwrap();
    assert_eq!(listed["folders"].as_array().unwrap().len(), 1);
    assert_eq!(listed["folders"][0]["title"], "Policies");
    assert_eq!(
        invoke(db, &job, "list_folder", json!({"folder":s.hr}))
            .await
            .unwrap()["state"],
        "not_found"
    );
    let read = invoke(db, &job, "read_document", json!({"id":s.returns}))
        .await
        .unwrap();
    assert!(
        read["passages"][0]["text"]
            .as_str()
            .unwrap()
            .contains("refund window is 30 days")
    );
    db.chat_finish(&job, "It's 30 days from delivery.", "complete")
        .await
        .unwrap();
    let answer: String = query(
        &db.db,
        "SELECT content FROM relay_chat_messages WHERE id=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "content")
    .unwrap();
    assert!(answer.starts_with("It's 30 days from delivery."));
    assert!(answer.contains("Library documents read for this answer:"));
    assert!(answer.contains(&format!(
        "[Returns policy · revision 1](/w/{}/?view=library&item={}&source_revision=1)",
        s.w, s.returns
    )));
    // The owner can see HR: in their private conversation the agent may use it…
    let private = chat(db, &s.owner, &s.w, vec![], vec![s.agent.clone()]).await;
    let mine = ask(&s, &s.owner, &private, "What is the support salary band?").await;
    assert_eq!(
        invoke(db, &mine, "read_document", json!({"id":s.salaries}))
            .await
            .unwrap()["state"],
        "available"
    );
    db.chat_finish(&mine, "40k.", "complete").await.unwrap();
    // …but not where a colleague who can't see it will read the answer.
    let shared = chat(
        db,
        &s.owner,
        &s.w,
        vec![s.member.id.clone()],
        vec![s.agent.clone()],
    )
    .await;
    let group = ask(&s, &s.owner, &shared, "What is the support salary band?").await;
    assert_eq!(
        invoke(db, &group, "read_document", json!({"id":s.salaries}))
            .await
            .unwrap()["state"],
        "not_found"
    );
    db.chat_finish(&group, "I can't share that here.", "complete")
        .await
        .unwrap();
    // Changing the folder's sharing after a read withholds the reply.
    let again = ask(&s, &s.owner, &private, "And the salary band again?").await;
    invoke(db, &again, "read_document", json!({"id":s.salaries}))
        .await
        .unwrap();
    assert!(db.chat_job_allowed(&again).await);
    db.library_share(&s.owner, &s.w, &s.hr, &share(2, "private", vec![], vec![]))
        .await
        .unwrap();
    assert!(!db.chat_job_allowed(&again).await);
    db.chat_finish(&again, "Salary band for support is 40k.", "complete")
        .await
        .unwrap();
    let withheld = query(
        &db.db,
        "SELECT content,state FROM relay_chat_messages WHERE id=$1",
        vec![again.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        withheld.try_get::<String>("", "state").unwrap(),
        "cancelled"
    );
    assert!(
        !withheld
            .try_get::<String>("", "content")
            .unwrap()
            .contains("40k")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_only_read_and_answer_staff_or_viewers_for_now() {
    let s = setup().await;
    for (level, use_for) in [
        ("edit_approval", "staff"),
        ("read", "customers"),
        ("read", "everyone"),
    ] {
        let mut wrong = grant(&s.agent, use_for);
        wrong.level = level.into();
        assert!(
            s.db.library_share(
                &s.owner,
                &s.w,
                &s.hr,
                &share(2, "private", vec![], vec![wrong])
            )
            .await
            .is_err(),
            "{level} for {use_for} must be refused"
        );
    }
    let detail = s.db.library_get(&s.owner, &s.w, &s.salaries).await.unwrap();
    assert_eq!(
        detail["agents"],
        json!([{"agent":s.agent,"level":"read","use_for":"viewers"}])
    );
    assert!(s.db.library_agent_has_items(&s.w, &s.agent).await.unwrap());
    let said: String = query(&s.db.db, "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='library_access_changed' AND target=$2", vec![s.w.clone().into(), s.hr.clone().into()])
        .await.unwrap().unwrap().try_get("", "summary").unwrap();
    assert_eq!(
        said,
        "Changed sharing of \"HR\": Pool agent reads it for people who can see it"
    );
    // Ask follows the folder's agents too.
    let ask = AskItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        agent: s.agent.clone(),
        question: "Summarise the bands".into(),
    };
    assert!(
        s.db.library_ask(&s.owner, &s.w, &s.salaries, &ask)
            .await
            .is_ok()
    );
    assert!(
        s.db.library_ask(&s.member, &s.w, &s.salaries, &ask)
            .await
            .is_err()
    );
}
