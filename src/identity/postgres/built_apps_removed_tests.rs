//! Schema 126 on a database that has built apps: their tables go, along with the
//! work-signal trigger on waiting app events, and the rest of the workspace stays.
use super::*;
use sea_orm_migration::MigratorTrait;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn removing_built_apps_drops_their_tables_and_keeps_the_workspace() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").expect("Set RELAY_TEST_DATABASE_URL");
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_test_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&url).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(url.as_str()).await.unwrap();
    super::migration::Migrator::up(&db.db, Some(125))
        .await
        .unwrap();
    db.db.execute_unprepared("INSERT INTO relay_users(id,provider,email) VALUES('mina','p-mina','mina@example.test');
        INSERT INTO relay_workspaces(id,name) VALUES('w1','Mehta');
        INSERT INTO relay_members(workspace,user_id,role) VALUES('w1','mina','Admin');
        INSERT INTO relay_built_apps(workspace,id,name,builder,audience,published,created,updated) VALUES('w1','leave','Leave','mina','everyone',1,1,1);
        INSERT INTO relay_built_app_files(workspace,app,version,path,content) VALUES('w1','leave',0,'index.html','<p>Leave</p>');
        INSERT INTO relay_built_app_records(workspace,app,stage,collection,id,data,created_by,updated_by,created,updated) VALUES('w1','leave','live','requests','r1','{\"days\":2}','mina','mina',1,1);
        INSERT INTO relay_built_app_events(workspace,app,collection,record,at) VALUES('w1','leave','requests','r1',1);").await.unwrap();

    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let left = query(
        &db.db,
        "SELECT count(*) AS n FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
          WHERE n.nspname=current_schema() AND c.relkind='r' AND c.relname LIKE 'relay\\_built\\_app%'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        left.try_get::<i64>("", "n").unwrap(),
        0,
        "every built-app table is gone"
    );
    let trigger = query(
        &db.db,
        "SELECT count(*) AS n FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid
          JOIN pg_namespace n ON n.oid=c.relnamespace
         WHERE n.nspname=current_schema() AND t.tgname='relay_app_event_work'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(trigger.try_get::<i64>("", "n").unwrap(), 0);
    let workspace = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_members WHERE workspace='w1' AND user_id='mina'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        workspace.try_get::<i64>("", "n").unwrap(),
        1,
        "the workspace and its people stay"
    );
}
