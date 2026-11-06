//! Library LB7: creators delete trashed items for good, one at a time or by emptying
//! Trash, never what is on hold; unfinished uploads' bytes are released once.
use super::super::*;
use super::folders::{page, setup};
use crate::identity::library::model::{ItemAction, UploadFile};
use crate::identity::postgres::Hold;

fn act(action: &str, revision: i64) -> ItemAction {
    ItemAction {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        action: action.into(),
        version: None,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn trash_is_emptied_for_good_except_held_items() {
    let s = setup().await;
    let db = &s.db;
    let saved = |title: &str| page(title, "Old notes.", None);
    let (old, draft, disputed) = (
        saved("Old minutes"),
        saved("Old draft"),
        saved("Disputed order"),
    );
    for item in [&old, &draft, &disputed] {
        db.library_save(&s.owner, &s.w, item).await.unwrap();
    }
    let theirs = saved("Member's note");
    db.library_save(&s.member, &s.w, &theirs).await.unwrap();
    // Only trashed items, and only by their creator.
    assert!(
        db.library_delete_forever(&s.owner, &s.w, &old.id)
            .await
            .unwrap_err()
            .to_string()
            .contains("Move it to Trash first")
    );
    for item in [&old, &draft, &disputed] {
        db.library_action(&s.owner, &s.w, &item.id, &act("trash", 1))
            .await
            .unwrap();
    }
    db.library_action(&s.member, &s.w, &theirs.id, &act("trash", 1))
        .await
        .unwrap();
    assert!(
        db.library_delete_forever(&s.member, &s.w, &old.id)
            .await
            .is_err()
    );
    // A hold placed while it is in Trash keeps it.
    db.library_hold_set(
        &s.owner,
        &s.w,
        &disputed.id,
        &Hold {
            reason: Some("Customer dispute".into()),
        },
    )
    .await
    .unwrap();
    assert!(
        db.library_delete_forever(&s.owner, &s.w, &disputed.id)
            .await
            .unwrap_err()
            .to_string()
            .contains("legal hold")
    );
    let files = db
        .library_delete_forever(&s.owner, &s.w, &old.id)
        .await
        .unwrap();
    assert!(files.is_empty(), "a page has no stored file");
    assert!(
        db.library_action(&s.owner, &s.w, &old.id, &act("restore", 3))
            .await
            .unwrap_err()
            .to_string()
            .contains("deleted for good")
    );
    // Emptying Trash takes the rest of the owner's, not the held one nor the member's.
    let (deleted, _) = db.library_empty_trash(&s.owner, &s.w).await.unwrap();
    assert_eq!(deleted, 1);
    let trash = db
        .library_list(&s.owner, &s.w, "trash", "", "", 0)
        .await
        .unwrap();
    let left: Vec<&str> = trash["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert_eq!(left, ["Disputed order"]);
    let member_trash = db
        .library_list(&s.member, &s.w, "trash", "", "", 0)
        .await
        .unwrap();
    assert_eq!(member_trash["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        db.library_empty_trash(&s.owner, &s.w).await.unwrap().0,
        0,
        "nothing more to empty"
    );
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
    assert!(
        said.contains("Deleted \"Old minutes\" for good from Trash"),
        "{said}"
    );
    assert!(
        said.contains("Emptied Trash: 1 item deleted for good"),
        "{said}"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn bytes_of_cancelled_or_unfinished_uploads_are_released_once() {
    let s = setup().await;
    let db = &s.db;
    let start = |title: &str| UploadFile {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        title: title.into(),
        size: 4,
        checksum: "a".repeat(64),
    };
    let (cancelled, abandoned, waiting) = (
        start("cancelled.txt"),
        start("abandoned.txt"),
        start("waiting.txt"),
    );
    for upload in [&cancelled, &abandoned, &waiting] {
        db.library_upload_start(&s.owner, &s.w, upload)
            .await
            .unwrap();
    }
    db.library_upload_cancel(&s.owner, &s.w, &cancelled.id)
        .await
        .unwrap();
    execute(
        &db.db,
        "UPDATE relay_library_uploads SET expires=1 WHERE id=$1",
        vec![abandoned.id.clone().into()],
    )
    .await
    .unwrap();
    let mut gone = db.library_orphaned_uploads(now()).await.unwrap();
    gone.sort();
    let mut expected = vec![
        format!("library/{}/{}", s.w, cancelled.id),
        format!("library/{}/{}", s.w, abandoned.id),
    ];
    expected.sort();
    assert_eq!(gone, expected, "the waiting upload keeps its bytes");
    assert!(
        db.library_orphaned_uploads(now()).await.unwrap().is_empty(),
        "each once"
    );
}
