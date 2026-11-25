//! File versions (schema 152): replacing a file adds a version from the revision read,
//! History lists and restores them, the quota counts every kept object, and dropped or
//! deleted versions hand their stored objects back for removal.
use super::super::{tests::*, *};
use super::folders::share;
use crate::identity::library::model::*;

async fn upload(
    db: &PgStore,
    user: &User,
    w: &str,
    text: &str,
    replaces: Option<(&str, i64)>,
) -> Result<String> {
    let input = UploadFile {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        title: "price-list.txt".into(),
        size: text.len() as i64,
        checksum: checksum(text.as_bytes()),
        replaces: replaces.map(|(item, revision)| Replaces {
            item: item.into(),
            revision,
        }),
    };
    db.library_upload_start(user, w, &input).await?;
    let stored = db
        .library_upload_finish(user, w, &input.id, "text/plain", text, "ready")
        .await?;
    // Finishing again is the same file, not another version.
    let again = db
        .library_upload_finish(user, w, &input.id, "text/plain", "ignored", "ready")
        .await?;
    assert_eq!(stored["revision"], again["revision"]);
    Ok(stored["id"].as_str().unwrap().to_owned())
}
fn restore(revision: i64, version: i64) -> ItemAction {
    ItemAction {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        action: "restore_version".into(),
        version: Some(version),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_file_is_replaced_by_its_editors_and_restored_from_history() {
    let db = fixture().await;
    let owner = user(&db, "file-owner").await;
    let viewer = user(&db, "file-viewer").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &viewer, "Member").await;
    let id = upload(&db, &owner, &w, "Desk: 1200", None).await.unwrap();
    db.library_share(
        &owner,
        &w,
        &id,
        &share(1, "selected", vec![(&viewer, "view")], vec![]),
    )
    .await
    .unwrap();
    // A viewer can't replace it, and an editor only from the revision they read.
    assert!(
        upload(&db, &viewer, &w, "Desk: 1", Some((&id, 1)))
            .await
            .is_err()
    );
    assert!(
        upload(&db, &owner, &w, "Desk: 1", Some((&id, 7)))
            .await
            .unwrap_err()
            .is::<Conflict>()
    );
    let same = upload(&db, &owner, &w, "Desk: 1250", Some((&id, 1)))
        .await
        .unwrap();
    assert_eq!(same, id, "the same item, not a second file");
    let now = db.library_file(&owner, &w, &id).await.unwrap();
    assert_eq!(
        (now["revision"].as_i64(), now["text"].as_str()),
        (Some(2), Some("Desk: 1250"))
    );
    let first_key = db.library_file_version(&owner, &w, &id, 1).await.unwrap()["object_key"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(now["object_key"], first_key);
    // A second replacement from the old revision doesn't overwrite the new file.
    assert!(
        upload(&db, &owner, &w, "Desk: 9", Some((&id, 1)))
            .await
            .unwrap_err()
            .is::<Conflict>()
    );
    let history = db.library_versions(&owner, &w, &id).await.unwrap();
    let sizes: Vec<_> = history["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["revision"].as_i64().unwrap(), v["size"].as_i64().unwrap()))
        .collect();
    assert_eq!(sizes, [(2, 10), (1, 10)]);
    // Viewers read the current file only.
    assert!(db.library_versions(&viewer, &w, &id).await.is_err());
    assert!(db.library_file_version(&viewer, &w, &id, 1).await.is_err());
    assert!(db.library_file_version(&viewer, &w, &id, 2).await.is_ok());
    // Restoring the first version is the next revision, with the first object.
    db.library_action(&owner, &w, &id, &restore(2, 1))
        .await
        .unwrap();
    let back = db.library_file(&owner, &w, &id).await.unwrap();
    assert_eq!(
        (
            back["revision"].as_i64(),
            back["text"].as_str(),
            back["object_key"].as_str()
        ),
        (Some(3), Some("Desk: 1200"), Some(first_key.as_str()))
    );
    // Both objects count once towards storage, however many versions name them.
    assert_eq!(
        super::super::library::file_versions::storage_used(&db.db, &w)
            .await
            .unwrap(),
        20
    );
    let found = db
        .library_list(&owner, &w, "all", "1200", "", 0)
        .await
        .unwrap();
    assert_eq!(found["items"].as_array().unwrap().len(), 1, "re-indexed");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn dropped_and_deleted_versions_hand_back_their_stored_objects() {
    let db = fixture().await;
    let owner = user(&db, "file-keeper").await;
    let w = workspace(&db, &owner).await;
    let id = upload(&db, &owner, &w, "v1", None).await.unwrap();
    let first = db.library_file(&owner, &w, &id).await.unwrap()["object_key"]
        .as_str()
        .unwrap()
        .to_owned();
    for revision in 1..=20 {
        upload(
            &db,
            &owner,
            &w,
            &format!("v{}", revision + 1),
            Some((&id, revision)),
        )
        .await
        .unwrap();
    }
    // Twenty-one versions were written and twenty are kept: the first object goes, once.
    let kept = db.library_versions(&owner, &w, &id).await.unwrap()["versions"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(kept, 20);
    assert_eq!(db.library_discarded_objects().await.unwrap(), [first]);
    assert!(db.library_discarded_objects().await.unwrap().is_empty());
    // Deleting the file for good returns every object its versions still name.
    let trash = ItemAction {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 21,
        action: "trash".into(),
        version: None,
    };
    db.library_action(&owner, &w, &id, &trash).await.unwrap();
    let files = db.library_delete_forever(&owner, &w, &id).await.unwrap();
    assert_eq!(files.len(), 20);
    assert_eq!(
        super::super::library::file_versions::storage_used(&db.db, &w)
            .await
            .unwrap(),
        0
    );
}
