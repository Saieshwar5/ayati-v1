use super::{tests::*, *};
use crate::identity::library::model::*;
use serde_json::json;
fn page(title: &str) -> SavePage {
    SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: title.into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Approved support instructions: offer a replacement after checking the purchase."}]}]}),
    }
}
fn share(revision: i64, visibility: &str, people: Vec<(&User, &str)>) -> ShareItem {
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
        agents: vec![],
    }
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn library_private_pages_and_item_permissions_are_independent_of_roles() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let writer = user(&db, "writer").await;
    let viewer = user(&db, "viewer").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &writer, "User").await;
    join(&db, &owner, &w, &viewer, "User").await;
    let input = page("Private working plan");
    let id = &input.id;
    db.library_save(&writer, &w, &input).await.unwrap();
    assert!(db.library_get(&owner, &w, id).await.is_err());
    assert!(
        db.library_list(&owner, &w, "all", "", 0).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut grants = share(1, "selected", vec![(&viewer, "view"), (&owner, "edit")]);
    db.library_share(&writer, &w, id, &grants).await.unwrap();
    db.library_share(&writer, &w, id, &grants).await.unwrap(); // exact replay
    assert_eq!(
        db.library_get(&viewer, &w, id).await.unwrap()["editable"],
        false
    );
    assert!(
        db.library_share(&owner, &w, id, &share(2, "workspace", vec![]))
            .await
            .is_err()
    );
    let mut changed = page("Attempted viewer edit");
    changed.id = id.clone();
    changed.revision = 1;
    assert!(db.library_save(&viewer, &w, &changed).await.is_err());
    grants.request = uuid::Uuid::new_v4().to_string();
    assert!(
        db.library_share(&writer, &w, id, &grants)
            .await
            .unwrap_err()
            .is::<Conflict>()
    );
    db.library_share(&writer, &w, id, &share(2, "private", vec![]))
        .await
        .unwrap();
    assert!(db.library_get(&viewer, &w, id).await.is_err());
    let other = workspace(&db, &owner).await;
    assert!(db.library_get(&writer, &other, id).await.is_err());
    assert!(db.library_versions(&viewer, &w, id).await.is_err());
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn library_concurrent_saves_replay_history_and_restore_preserve_acknowledged_content() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let w = workspace(&db, &owner).await;
    let input = page("Plan");
    let first = db.library_save(&owner, &w, &input).await.unwrap();
    assert_eq!(db.library_save(&owner, &w, &input).await.unwrap(), first);
    let mut a = page("One");
    a.id = input.id.clone();
    a.revision = 1;
    let mut b = page("Two");
    b.id = input.id.clone();
    b.revision = 1;
    let (left, right) = tokio::join!(
        db.library_save(&owner, &w, &a),
        db.library_save(&owner, &w, &b)
    );
    assert_ne!(left.is_ok(), right.is_ok());
    let accepted = if left.is_ok() { &a } else { &b };
    assert_eq!(
        db.library_save(&owner, &w, accepted).await.unwrap()["revision"],
        2
    );
    assert_eq!(
        db.library_versions(&owner, &w, &input.id).await.unwrap()["versions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let action = ItemAction {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 2,
        action: "restore_version".into(),
        version: Some(1),
    };
    db.library_action(&owner, &w, &input.id, &action)
        .await
        .unwrap();
    db.library_action(&owner, &w, &input.id, &action)
        .await
        .unwrap();
    assert_eq!(
        db.library_get(&owner, &w, &input.id).await.unwrap()["title"],
        "Plan"
    );
    assert_eq!(
        db.library_get(&owner, &w, &input.id).await.unwrap()["revision"],
        3
    );
    assert!(db.library_save(&owner, &w, accepted).await.is_ok()); // exact old acknowledgement, no overwrite
    assert_eq!(
        db.library_get(&owner, &w, &input.id).await.unwrap()["revision"],
        3
    );
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn library_member_removal_and_reinvitation_do_not_reactivate_private_ownership_or_grants() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let writer = user(&db, "writer").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &writer, "User").await;
    let input = page("Private");
    db.library_save(&writer, &w, &input).await.unwrap();
    let shared = page("Shared");
    db.library_save(&owner, &w, &shared).await.unwrap();
    db.library_share(
        &owner,
        &w,
        &shared.id,
        &share(1, "selected", vec![(&writer, "edit")]),
    )
    .await
    .unwrap();
    let team_page = page("Workspace instructions");
    db.library_save(&writer, &w, &team_page).await.unwrap();
    db.library_share(&writer, &w, &team_page.id, &share(1, "workspace", vec![]))
        .await
        .unwrap();
    db.change(&owner, &w, &writer.id, None).await.unwrap();
    assert!(db.library_get(&writer, &w, &input.id).await.is_err());
    join(&db, &owner, &w, &writer, "User").await;
    assert!(db.library_get(&writer, &w, &input.id).await.is_err());
    assert!(db.library_get(&writer, &w, &shared.id).await.is_err());
    assert!(db.library_get(&owner, &w, &input.id).await.is_err());
    assert_eq!(
        db.library_get(&owner, &w, &team_page.id).await.unwrap()["steward"],
        Value::Null
    );
    db.library_action(
        &owner,
        &w,
        &team_page.id,
        &ItemAction {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            action: "claim".into(),
            version: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.library_get(&owner, &w, &team_page.id).await.unwrap()["manageable"],
        true
    );
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn library_file_sessions_reconcile_cancel_and_reserve_quota() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let w = workspace(&db, &owner).await;
    let bytes = b"Business instructions";
    let input = UploadFile {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        title: "reference.txt".into(),
        size: bytes.len() as i64,
        checksum: checksum(bytes),
    };
    db.library_upload_start(&owner, &w, &input).await.unwrap();
    db.library_upload_start(&owner, &w, &input).await.unwrap();
    db.library_upload_finish(
        &owner,
        &w,
        &input.id,
        "text/plain",
        "Business instructions",
        "ready",
    )
    .await
    .unwrap();
    db.library_upload_finish(
        &owner,
        &w,
        &input.id,
        "text/plain",
        "Changed text ignored",
        "ready",
    )
    .await
    .unwrap();
    assert_eq!(
        db.library_file(&owner, &w, &input.id).await.unwrap()["text"],
        "Business instructions"
    );
    let mut pending = input;
    pending.id = uuid::Uuid::new_v4().to_string();
    pending.request = uuid::Uuid::new_v4().to_string();
    db.library_upload_start(&owner, &w, &pending).await.unwrap();
    db.library_upload_cancel(&owner, &w, &pending.id)
        .await
        .unwrap();
    assert!(
        db.library_upload_finish(&owner, &w, &pending.id, "text/plain", "x", "ready")
            .await
            .is_err()
    );
    execute(
        &db.db,
        "UPDATE relay_library_items SET size=$3 WHERE workspace=$1 AND id=$2",
        vec![
            w.clone().into(),
            db.library_list(&owner, &w, "all", "", 0).await.unwrap()["items"][0]["id"]
                .as_str()
                .unwrap()
                .into(),
            (WORKSPACE_QUOTA - 1).into(),
        ],
    )
    .await
    .unwrap();
    pending.id = uuid::Uuid::new_v4().to_string();
    pending.request = uuid::Uuid::new_v4().to_string();
    assert!(db.library_upload_start(&owner, &w, &pending).await.is_err());
    // Pending file reservations and native pages share the same item budget.
    execute(
        &db.db,
        "UPDATE relay_library_items SET size=21 WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap();
    execute(&db.db,"INSERT INTO relay_library_uploads(workspace,id,actor,title,size,checksum,object_key,expires) SELECT $1,gen_random_uuid()::text,$2,'reserved.txt',1,$3,'library/capacity/'||n::text,$4 FROM generate_series(1,999) n",vec![w.clone().into(),owner.id.clone().into(),checksum(b"x").into(),(now()+86400).into()]).await.unwrap();
    assert!(
        db.library_save(&owner, &w, &page("Cannot exceed reserved capacity"))
            .await
            .is_err()
    );
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn library_sources_are_private_revision_bound_and_fence_claimed_replies_after_revocation() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let member = user(&db, "member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "User").await;
    let aid = uuid::Uuid::new_v4().to_string();
    db.save_shared_agent(
        &owner,
        &w,
        &aid,
        "Support",
        "Answer support questions",
        "fixture",
    )
    .await
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET revision=1,settings='{}' WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), aid.clone().into()],
    )
    .await
    .unwrap();
    db.chat_grant(&owner, &w, &member.id, &aid, true)
        .await
        .unwrap();
    let page = page("Support policy");
    db.library_save(&owner, &w, &page).await.unwrap();
    let mut sharing = share(1, "private", vec![]);
    sharing.agents = vec![aid.clone()];
    db.library_share(&owner, &w, &page.id, &sharing)
        .await
        .unwrap();
    let ask = AskItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        agent: aid,
        question: "How should I handle a return?".into(),
    };
    assert!(db.library_ask(&member, &w, &page.id, &ask).await.is_err());
    let result = db.library_ask(&owner, &w, &page.id, &ask).await.unwrap();
    assert_eq!(
        db.library_ask(&owner, &w, &page.id, &ask).await.unwrap(),
        result
    );
    assert!(
        db.check_chat(&member, &w, result["chat"].as_str().unwrap())
            .await
            .is_err()
    );
    let job = db.claim_job().await.unwrap().unwrap();
    assert!(db.chat_job_allowed(&job).await);
    assert!(
        db.library_job_context(&job).await.unwrap()[0]["text"]
            .as_str()
            .unwrap()
            .contains("replacement")
    );
    // History is bounded while a version supplied to Chat remains verifiable.
    for revision in 1..=55 {
        let mut next = super::library_tests::page(&format!("Policy update {revision}"));
        next.id = page.id.clone();
        next.revision = revision;
        next.document = json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":format!("New policy {revision}")}]}]});
        db.library_save(&owner, &w, &next).await.unwrap();
    }
    assert!(
        db.library_job_context(&job).await.unwrap()[0]["text"]
            .as_str()
            .unwrap()
            .contains("replacement")
    );
    assert!(db.library_version(&owner, &w, &page.id, 1).await.is_ok());
    assert!(db.library_version(&owner, &w, &page.id, 2).await.is_err());
    let retained: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_library_versions WHERE workspace=$1 AND item=$2",
        vec![w.clone().into(), page.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(retained, 51);
    db.library_share(&owner, &w, &page.id, &share(2, "private", vec![]))
        .await
        .unwrap();
    assert!(!db.chat_job_allowed(&job).await);
    assert!(db.library_job_context(&job).await.is_err());
    db.chat_finish(&job, "Private extracted secret", "complete")
        .await
        .unwrap();
    let row = query(
        &db.db,
        "SELECT content,state FROM relay_chat_messages WHERE id=$1",
        vec![job.reply.into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "cancelled");
    assert!(
        !row.try_get::<String>("", "content")
            .unwrap()
            .contains("secret")
    );
}
