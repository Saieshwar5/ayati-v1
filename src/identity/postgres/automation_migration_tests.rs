use super::{tests::*, *};
use sea_orm_migration::MigratorTrait;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_upgrade_preserves_v26_profiles_and_sample_receipts_without_dispatch() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_automation_upgrade_{}", uuid::Uuid::new_v4().simple());
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
    migration::Migrator::up(&db.db, Some(26)).await.unwrap();
    let owner = user(&db, "automation-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let input = crate::identity::rehearsals::model::Start {
        id: uuid::Uuid::new_v4().to_string(),
        agent_revision: 1,
        task: "Existing saved sample".into(),
        sources: vec![],
    };
    // A completed v26 receipt, written as the v26 controller saved it.
    let profile = db.managed_agent(&owner, &w, &a, true).await.unwrap();
    execute(&db.db,"INSERT INTO relay_agent_rehearsals(workspace,id,agent,actor,request_hash,input,profile,state,output,finished_ms) VALUES($1,$2,$3,$4,'v26',$5,$6,'completed','Saved before upgrade',1)",
        vec![w.clone().into(),input.id.clone().into(),a.clone().into(),owner.id.clone().into(),serde_json::to_value(&input).unwrap().into(),profile.into()]).await.unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    assert_eq!(
        db.rehearsal(&owner, &w, &a, &input.id).await.unwrap()["output"],
        "Saved before upgrade"
    );
    assert_eq!(db.company_counts(&owner, &w).await.unwrap()["jobs"], 0);
    assert!(
        !db.post_targets(&owner, &w, &a).await.unwrap()["execution_available"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 0);
    assert!(
        db.chat_messages(&owner, &w, &c).await.unwrap()["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
