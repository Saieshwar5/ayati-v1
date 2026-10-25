use super::{tests::*, *};
use sea_orm_migration::MigratorTrait;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn connection_upgrade_preserves_owner_rules_without_delegating_to_members() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_connection_upgrade_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&url).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(18)).await.unwrap();
    let owner = user(&db, "upgrade-owner").await;
    let member = user(&db, "upgrade-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "User").await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    execute(&db.db, "INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint) VALUES($1,'account','orders','{}','old-reviewed')", vec![w.clone().into()]).await.unwrap();
    execute(&db.db, "INSERT INTO relay_control_rules(workspace,agent,connection,operation,mode,approver,revision,chat) VALUES($1,$2,'account','order.read','allow',$3,4,$4)", vec![w.clone().into(), agent.clone().into(), owner.id.clone().into(), chat.clone().into()]).await.unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    assert!(db.control_enabled(&w, &agent, &chat).await.unwrap());
    let snapshot = db.control_snapshot(&owner, &w, &agent).await.unwrap();
    assert_eq!(snapshot["rules"][0]["configured_by"], owner.id);
    assert_eq!(snapshot["rules"][0]["revision"], 4);
    assert!(snapshot["rules"][0]["delegation_revision"].is_null());
    assert!(!db.can_create_workspace_agent(&member, &w).await.unwrap());
    assert!(
        db.workspace_connections(&owner, &w).await.unwrap()["delegations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    db.change(&owner, &w, &member.id, Some("Owner"))
        .await
        .unwrap();
    db.change(&member, &w, &owner.id, Some("User"))
        .await
        .unwrap();
    assert!(!db.control_enabled(&w, &agent, &chat).await.unwrap());
    db.change(&member, &w, &owner.id, Some("Owner"))
        .await
        .unwrap();
    assert!(!db.control_enabled(&w, &agent, &chat).await.unwrap());
}
