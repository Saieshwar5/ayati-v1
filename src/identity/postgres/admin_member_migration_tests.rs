use super::*;
use sea_orm_migration::MigratorTrait;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_43_renames_owner_and_user_without_changing_access() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_admin_member_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(scoped.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(42)).await.unwrap();
    db.db
        .execute_unprepared(
            "INSERT INTO relay_users(id,provider,email) VALUES('lead','p_lead','lead@test'),('staff','p_staff','staff@test');
             INSERT INTO relay_workspaces(id,name) VALUES('shop','Shop');
             INSERT INTO relay_members(workspace,user_id,role) VALUES('shop','lead','Owner'),('shop','staff','User');
             INSERT INTO relay_invitations(id,workspace,email,role,inviter,expires,status)
               VALUES('invite','shop','new@test','User','lead',9999999999,'pending');",
        )
        .await
        .unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();

    assert_eq!(db.role("shop", "lead").await.unwrap(), "Admin");
    assert_eq!(db.role("shop", "staff").await.unwrap(), "Member");
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
    let renamed = rows(
        &db.db,
        "SELECT target FROM relay_audit WHERE action='role_renamed' ORDER BY target",
        vec![],
    )
    .await
    .unwrap()
    .iter()
    .map(|row| row.try_get::<String>("", "target").unwrap())
    .collect::<Vec<_>>();
    assert_eq!(renamed, ["Owner -> Admin", "User -> Member"]);
    for old in ["Owner", "User"] {
        assert!(
            db.db
                .execute_unprepared(&format!(
                    "UPDATE relay_members SET role='{old}' WHERE user_id='staff'"
                ))
                .await
                .is_err(),
            "{old} must be refused"
        );
    }

    // The rename leaves role-change safety triggers switched back on.
    let disabled: i64 = query(
        &db.db,
        "SELECT count(*) AS count FROM pg_trigger WHERE tgrelid='relay_members'::regclass AND NOT tgisinternal AND tgenabled='D'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "count")
    .unwrap();
    assert_eq!(disabled, 0);

    // The renamed admin keeps every admin right and the member gains none.
    let lead = User {
        id: "lead".into(),
        email: "lead@test".into(),
        display_name: None,
    };
    let staff = User {
        id: "staff".into(),
        email: "staff@test".into(),
        display_name: None,
    };
    db.reserve_invite(&lead, "shop", "second@test", "Member")
        .await
        .unwrap();
    assert!(
        db.reserve_invite(&staff, "shop", "third@test", "Member")
            .await
            .is_err()
    );
    assert!(
        db.change(&staff, "shop", "staff", Some("Admin"), None)
            .await
            .is_err()
    );
}
