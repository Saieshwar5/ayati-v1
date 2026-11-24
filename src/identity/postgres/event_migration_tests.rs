use super::{tests::*, *};
use sea_orm_migration::MigratorTrait;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_upgrade_keeps_v27_saved_schedules_without_registering_sources_or_dispatching() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_event_upgrade_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let mut db = PgStore::connect(scoped.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(27)).await.unwrap();
    assert!(db.verify_schema().await.is_err());
    db.pooled = true;
    let owner = user(&db, "event-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    // A one-time schedule as the v27 controller saved it.
    let id = uuid::Uuid::new_v4().to_string();
    execute(&db.db, "INSERT INTO relay_agent_schedules(workspace,id,agent,principal,chat,name,content,first_ms,interval_seconds,timezone,request_hash,next_ms) VALUES($1,$2,$3,$4,$5,'Delivery review','Review delivery exceptions.',$6,NULL,'Asia/Kolkata',$2,$6)",
        vec![w.clone().into(), id.clone().into(), a.clone().into(), owner.id.clone().into(), c.clone().into(), (now() * 1000 + 60000).into()]).await.unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    // It is a task now (schema 147), its one run still booked.
    let task = query(&db.db, "SELECT t.outcome,(SELECT k.kind FROM relay_task_wakeups k WHERE k.task=t.id AND k.state='pending') AS next FROM relay_tasks t WHERE t.schedule=$1",
        vec![id.into()]).await.unwrap().unwrap();
    assert_eq!(task.try_get::<Option<String>>("", "outcome").unwrap(), None);
    assert_eq!(task.try_get::<String>("", "next").unwrap(), "follow_up");
    assert_eq!(
        db.event_triggers(&owner, &w, &a, &[]).await.unwrap()["triggers"],
        serde_json::json!([])
    );
    assert_eq!(db.dispatch_events().await.unwrap(), 0);
    assert_eq!(db.company_counts(&owner, &w).await.unwrap()["jobs"], 0);
}
