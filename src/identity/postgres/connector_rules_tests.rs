use super::*;
use sea_orm_migration::MigratorTrait;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_47_keeps_the_strictest_delegated_limit_and_asks_waiting_work_again() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_connector_safety_{}", uuid::Uuid::new_v4().simple());
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
    migration::Migrator::up(&db.db, Some(46)).await.unwrap();
    db.db
        .execute_unprepared(
            r#"INSERT INTO relay_users(id,provider,email) VALUES('lead','p_lead','lead@test'),('staff','p_staff','staff@test'),('other','p_other','other@test');
             INSERT INTO relay_workspaces(id,name) VALUES('shop','Shop');
             INSERT INTO relay_members(workspace,user_id,role) VALUES('shop','lead','Admin'),('shop','staff','Member'),('shop','other','Member');
             INSERT INTO relay_shared_agents(workspace,id,name,role,model,owner_id,creator_id,revision) VALUES('shop','atlas','Atlas','Orders','m','staff','staff',1);
             INSERT INTO relay_chats(id,workspace,name,creator) VALUES('desk','shop','Desk','staff');
             INSERT INTO relay_control_systems(workspace,server,approved,revision) VALUES('shop','orders',true,1);
             INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint) VALUES('shop','books','orders',
               '{"id":"orders","name":"Orders","operations":[{"id":"order.read","title":"Read","tool":"read","effect":"read","schema":{}},{"id":"order.update","title":"Update","tool":"update","effect":"write","schema":{}}]}','fp');
             INSERT INTO relay_control_delegations(workspace,connection,user_id,operation,ceiling,approver,revision)
               VALUES('shop','books','staff','order.update','allow','lead',1),('shop','books','other','order.update','ask','lead',1);
             INSERT INTO relay_control_rules(workspace,agent,connection,operation,mode,approver,revision,chat,configured_by,delegation_revision)
               VALUES('shop','atlas','books','order.update','allow','lead',3,'desk','staff',1);
             INSERT INTO relay_control_actions(id,workspace,agent,chat,job,requester,request_id,connection,operation,arguments,title,effect,rule_revision,connection_revision,approver,state,created,updated)
               VALUES('waiting','shop','atlas','desk','job','staff','r','books','order.update','{}','Update','write',3,1,'lead','pending',1,1);"#,
        )
        .await
        .unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();

    let one = |sql: &'static str| {
        let db = &db;
        async move {
            query(&db.db, sql, vec![])
                .await
                .unwrap()
                .unwrap()
                .try_get::<String>("", "v")
                .unwrap()
        }
    };
    // Staff could allow, another person only ask: the workspace rule takes Ask first.
    assert_eq!(
        one("SELECT mode AS v FROM relay_connector_rules WHERE operation='order.update'").await,
        "ask"
    );
    assert_eq!(
        one("SELECT agent AS v FROM relay_agent_connections").await,
        "atlas"
    );
    // The agent's own Allow stays, but the workspace rule bounds it.
    assert_eq!(
        one("SELECT mode AS v FROM relay_control_effective_rules WHERE operation='order.update'")
            .await,
        "ask"
    );
    assert_eq!(
        one("SELECT mode AS v FROM relay_control_effective_rules WHERE operation='order.read'")
            .await,
        "allow"
    );
    assert_eq!(
        one(
            "SELECT approver AS v FROM relay_control_effective_rules WHERE operation='order.update'"
        )
        .await,
        "staff"
    );
    assert_eq!(
        one("SELECT state AS v FROM relay_control_actions WHERE id='waiting'").await,
        "invalidated"
    );
    for gone in ["relay_control_delegations", "relay_control_approvers"] {
        assert!(
            db.db
                .execute_unprepared(&format!("SELECT 1 FROM {gone}"))
                .await
                .is_err(),
            "{gone}"
        );
    }
}
