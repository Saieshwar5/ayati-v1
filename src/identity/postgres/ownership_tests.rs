use super::tests::{fixture, join, user, workspace};
use super::*;
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn equal_owners_concurrent_removal_keeps_one_owner_and_revokes_access() {
    let db = fixture().await;
    let a = user(&db, "first").await;
    let b = user(&db, "second").await;
    let w = workspace(&db, &a).await;
    join(&db, &a, &w, &b, "Member").await;
    assert!(db.people(&b, &w).await.is_err());
    assert!(db.workspace_settings(&b, &w).await.is_err());
    assert!(db.workspace_activity(&b, &w).await.is_err());
    assert!(
        db.reserve_invite(&b, &w, "third@test", "Member")
            .await
            .is_err()
    );
    assert!(db.change(&b, &w, &b.id, Some("Admin")).await.is_err());
    db.change(&a, &w, &b.id, Some("Admin")).await.unwrap();
    db.rename_workspace(&b, &w, "Renamed", "Test business")
        .await
        .unwrap();
    let (one, two) = tokio::join!(
        db.change(&a, &w, &a.id, Some("Member")),
        db.change(&b, &w, &b.id, None)
    );
    assert_ne!(one.is_ok(), two.is_ok());
    let owner = if one.is_ok() { &b } else { &a };
    assert!(db.change(owner, &w, &owner.id, None).await.is_err());
    assert!(
        db.change(owner, &w, &owner.id, Some("Member"))
            .await
            .is_err()
    );
    assert!(
        !db.workspace_activity(owner, &w).await.unwrap()["activity"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn migration_preserves_legacy_memberships_without_promoting_admins() {
    use sea_orm_migration::MigratorTrait;
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_role_upgrade_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&url).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(7)).await.unwrap();
    db.db.execute_unprepared("INSERT INTO relay_users(id,provider,email) VALUES('owner','old_owner','owner@test'),('admin','old_admin','admin@test'); INSERT INTO relay_workspaces(id,name) VALUES('old','Existing'); INSERT INTO relay_members(workspace,user_id,role) VALUES('old','owner','Owner'),('old','admin','Admin'); INSERT INTO relay_chat_access VALUES('old','admin','agent'); INSERT INTO relay_invitations(id,workspace,email,role,inviter,expires,status) VALUES('invite','old','pending@test','Admin','owner',9999999999,'pending');").await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    assert_eq!(db.role("old", "owner").await.unwrap(), "Admin");
    assert_eq!(db.role("old", "admin").await.unwrap(), "Member");
    assert!(
        query(
            &db.db,
            "SELECT agent FROM relay_chat_access WHERE user_id='admin'",
            vec![]
        )
        .await
        .unwrap()
        .is_some()
    );
    let invitation: String = query(
        &db.db,
        "SELECT role FROM relay_invitations WHERE id='invite'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "role")
    .unwrap();
    assert_eq!(invitation, "Member");
    assert!(
        db.db
            .execute_unprepared("UPDATE relay_members SET role='Owner' WHERE user_id='admin'")
            .await
            .is_err()
    );
}
