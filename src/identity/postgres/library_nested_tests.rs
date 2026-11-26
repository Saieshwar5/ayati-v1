//! Schema 153: folders inside folders follow the nearest folder with its own sharing,
//! and admins can open everything, which the Audit log records.
use super::super::{tests::*, *};
use super::folders::{folder, page, share};
use crate::identity::library::model::*;
use serde_json::json;

async fn subfolder(
    db: &PgStore,
    user: &User,
    w: &str,
    title: &str,
    parent: &str,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    db.library_folder_create(
        user,
        w,
        &CreateFolder {
            id: id.clone(),
            request: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            folder: Some(parent.into()),
        },
    )
    .await?;
    Ok(id)
}
fn act(action: &str, revision: i64) -> ItemAction {
    ItemAction {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        action: action.into(),
        version: None,
    }
}
fn moving(revision: i64, folder: Option<&str>) -> MoveItem {
    MoveItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        folder: folder.map(str::to_owned),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_subfolder_follows_its_folder_until_it_is_shared_itself() {
    let db = fixture().await;
    let owner = user(&db, "tree-owner").await;
    let member = user(&db, "tree-member").await;
    let other = user(&db, "tree-other").await;
    let w = workspace(&db, &owner).await;
    // The owner is an admin; use two members to see ordinary access.
    join(&db, &owner, &w, &member, "Member").await;
    join(&db, &owner, &w, &other, "Member").await;
    let sales = folder(&db, &member, &w, "Sales").await;
    let quotes = subfolder(&db, &member, &w, "Quotes", &sales).await.unwrap();
    let quote = page("Quote for Acme", "Desk 1250", Some(&quotes));
    db.library_save(&member, &w, &quote).await.unwrap();
    assert!(db.library_get(&other, &w, &quote.id).await.is_err());
    // Sharing the top folder reaches the page two levels down, and fences it.
    db.library_share(&member, &w, &sales, &share(1, "workspace", vec![], vec![]))
        .await
        .unwrap();
    let seen = db.library_get(&other, &w, &quote.id).await.unwrap();
    assert_eq!(
        (
            seen["folder_title"].as_str(),
            seen["source"].as_str(),
            seen["access_revision"].as_i64()
        ),
        (Some("Quotes"), Some(sales.as_str()), Some(2))
    );
    // Someone who only views the folder can't add a subfolder to it.
    assert!(subfolder(&db, &other, &w, "Mine", &sales).await.is_err());
    // The subfolder takes sharing of its own: now only chosen people open what is in it.
    db.library_share(&member, &w, &quotes, &share(2, "private", vec![], vec![]))
        .await
        .unwrap();
    assert!(db.library_get(&other, &w, &quote.id).await.is_err());
    assert!(db.library_get(&other, &w, &sales).await.is_ok());
    let own = db.library_get(&member, &w, &quote.id).await.unwrap();
    assert_eq!(
        (own["source"].as_str(), own["access_revision"].as_i64()),
        (Some(quotes.as_str()), Some(3))
    );
    // Following the folder again opens it back up.
    db.library_action(&member, &w, &quotes, &act("follow_folder", 1))
        .await
        .unwrap();
    assert!(db.library_get(&other, &w, &quote.id).await.is_ok());
    // The folder tree names every folder the person was given, with its parent.
    let tree = db.library_folders(&other, &w).await.unwrap();
    let named: Vec<(String, Value)> = tree["folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["title"].as_str().unwrap().to_owned(), f["folder"].clone()))
        .collect();
    assert_eq!(
        named,
        [
            ("Quotes".to_owned(), json!(sales)),
            ("Sales".to_owned(), Value::Null)
        ]
    );
    assert_eq!(tree["folders"][0]["editable"], false);
    // A folder lists what is directly in it; the top lists only top-level items.
    let titles = |v: Value| -> Vec<String> {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["title"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        titles(
            db.library_list(&member, &w, "all", "", "", 0)
                .await
                .unwrap()
        ),
        ["Sales"]
    );
    assert_eq!(
        titles(
            db.library_list(&member, &w, "all", "", &sales, 0)
                .await
                .unwrap()
        ),
        ["Quotes"]
    );
    assert_eq!(
        titles(
            db.library_list(&member, &w, "all", "", &quotes, 0)
                .await
                .unwrap()
        ),
        ["Quote for Acme"]
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn folders_move_without_loops_or_deep_nesting_and_holds_reach_down() {
    let db = fixture().await;
    let owner = user(&db, "move-owner").await;
    let w = workspace(&db, &owner).await;
    let a = folder(&db, &owner, &w, "A").await;
    let b = subfolder(&db, &owner, &w, "B", &a).await.unwrap();
    let c = subfolder(&db, &owner, &w, "C", &b).await.unwrap();
    // A can't go inside its own subfolder.
    assert!(
        db.library_move(&owner, &w, &a, &moving(1, Some(&c)))
            .await
            .is_err()
    );
    let d = subfolder(&db, &owner, &w, "D", &c).await.unwrap();
    let e = subfolder(&db, &owner, &w, "E", &d).await.unwrap();
    assert!(
        subfolder(&db, &owner, &w, "F", &e).await.is_err(),
        "five levels at most"
    );
    // A tree two deep doesn't fit under the fourth level either.
    let x = folder(&db, &owner, &w, "X").await;
    subfolder(&db, &owner, &w, "Y", &x).await.unwrap();
    assert!(
        db.library_move(&owner, &w, &x, &moving(1, Some(&d)))
            .await
            .is_err()
    );
    db.library_move(&owner, &w, &x, &moving(1, Some(&b)))
        .await
        .unwrap();
    // A page deep down is under its top folder's hold and keeping rule.
    let note = page("Deep note", "Kept", Some(&e));
    db.library_save(&owner, &w, &note).await.unwrap();
    db.library_keeping_set(
        &owner,
        &w,
        &b,
        &Keeping {
            keep_days: Some(30),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.library_keeping(&owner, &w, &note.id).await.unwrap()["folder_keep_days"],
        30
    );
    db.library_hold_set(
        &owner,
        &w,
        &a,
        &Hold {
            reason: Some("Audit".into()),
        },
    )
    .await
    .unwrap();
    assert!(
        db.library_action(&owner, &w, &note.id, &act("trash", 1))
            .await
            .is_err(),
        "held through a folder four levels up"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn admins_open_everything_and_the_audit_log_says_so() {
    let db = fixture().await;
    let admin = user(&db, "all-admin").await;
    let writer = user(&db, "all-writer").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &writer, "Member").await;
    let diary = page("Private plan", "Only mine", None);
    db.library_save(&writer, &w, &diary).await.unwrap();
    // Not in the admin's own views, but listed under Everything, which members can't ask for.
    let count = |v: Value| v["items"].as_array().unwrap().len();
    assert_eq!(
        count(db.library_list(&admin, &w, "all", "", "", 0).await.unwrap()),
        0
    );
    assert_eq!(
        count(
            db.library_list(&admin, &w, "everything", "", "", 0)
                .await
                .unwrap()
        ),
        1
    );
    assert!(
        db.library_list(&writer, &w, "everything", "", "", 0)
            .await
            .is_err()
    );
    // Opening it works, with full control, and is audited once however often they look.
    for _ in 0..2 {
        let seen = db.library_get(&admin, &w, &diary.id).await.unwrap();
        assert_eq!(
            (
                seen["editable"].as_bool(),
                seen["manageable"].as_bool(),
                seen["granted"].as_bool()
            ),
            (Some(true), Some(true), Some(false))
        );
    }
    let said: Vec<String> = rows(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='library_admin_opened'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.try_get("", "summary").unwrap())
    .collect();
    assert_eq!(
        said,
        ["Opened \"Private plan\" as an admin; it was not shared with them"]
    );
    // Looking inside a private folder and reading a page's history are audited as well.
    let private = folder(&db, &writer, &w, "Drafts").await;
    let draft = page("Draft", "Mine", Some(&private));
    db.library_save(&writer, &w, &draft).await.unwrap();
    db.library_list(&admin, &w, "all", "", &private, 0)
        .await
        .unwrap();
    db.library_versions(&admin, &w, &draft.id).await.unwrap();
    let opened: Vec<String> = rows(&db.db, "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='library_admin_opened' ORDER BY id", vec![w.clone().into()]).await.unwrap().into_iter().map(|r| r.try_get("", "summary").unwrap()).collect();
    assert_eq!(opened.len(), 3, "{opened:?}");
    assert!(
        opened[1].starts_with("Opened \"Drafts\"") && opened[2].starts_with("Opened \"Draft\""),
        "{opened:?}"
    );
    // What the writer opens themselves, or shares with the admin, isn't audited.
    db.library_get(&writer, &w, &diary.id).await.unwrap();
    db.library_share(
        &admin,
        &w,
        &diary.id,
        &share(1, "workspace", vec![], vec![]),
    )
    .await
    .unwrap();
    assert_eq!(
        db.library_get(&admin, &w, &diary.id).await.unwrap()["granted"],
        true
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn ten_thousand_items_in_nested_folders_list_and_search_within_a_second() {
    let db = fixture().await;
    let owner = user(&db, "scale-owner").await;
    let member = user(&db, "scale-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    // 100 top folders shared with the workspace, 4 levels of subfolders under each, and
    // 9,500 pages spread over the deepest ones: every page is four folders from its sharing.
    execute(&db.db, "INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,visibility,created,updated,folder) SELECT $1,'f'||l||'-'||n,'folder','Folder '||l||' '||n,$2,$2,CASE WHEN l=0 THEN 'workspace' ELSE 'private' END,1,1,CASE WHEN l=0 THEN NULL ELSE 'f'||(l-1)||'-'||n END FROM generate_series(0,4) l, generate_series(1,100) n ORDER BY l", vec![w.clone().into(), owner.id.clone().into()]).await.unwrap();
    execute(&db.db, "INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,document,text,created,updated,folder) SELECT $1,'p'||n,'page','Page '||n,$2,$2,'{\"type\":\"doc\"}','invoice '||n||' for desks',1,n,'f4-'||(1+n%100) FROM generate_series(1,9500) n", vec![w.clone().into(), owner.id.clone().into()]).await.unwrap();
    let kept: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_library_items WHERE workspace=$1 AND source LIKE 'f0-%'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(
        kept, 10_000,
        "every item knows the folder whose sharing it follows"
    );
    let timed = |view: &'static str, search: &'static str, folder: &'static str| {
        let (db, member, w) = (&db, &member, &w);
        async move {
            let start = std::time::Instant::now();
            let found = db
                .library_list(member, w, view, search, folder, 0)
                .await
                .unwrap();
            (found["items"].as_array().unwrap().len(), start.elapsed())
        }
    };
    let mut took = vec![];
    for (name, view, search, folder, expected) in [
        ("home", "all", "", "", 50),
        ("shared", "shared", "", "", 50),
        ("search", "all", "invoice 77 ", "", 1),
        ("folder", "all", "", "f4-7", 50),
    ] {
        let (found, time) = timed(view, search, folder).await;
        assert_eq!(found, expected, "{name}");
        took.push((name, time));
    }
    println!("Library with 10,000 items: {took:?}");
    for (name, time) in took {
        assert!(
            time < std::time::Duration::from_secs(1),
            "{name} took {time:?}"
        );
    }
    // Sharing one top folder separately moves only what is inside it.
    execute(
        &db.db,
        "UPDATE relay_library_items SET shares=true WHERE workspace=$1 AND id='f2-7'",
        vec![w.clone().into()],
    )
    .await
    .unwrap();
    let moved: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_library_items WHERE workspace=$1 AND source='f2-7'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(
        moved,
        3 + 95,
        "the folder, the two below it and their pages"
    );
}
