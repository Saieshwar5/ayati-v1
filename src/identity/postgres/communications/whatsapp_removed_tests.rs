//! Schema 114 on a database that used WhatsApp: the agent's live endpoint is
//! switched off as a new generation and the WhatsApp tables are gone.
use super::super::*;
use sea_orm_migration::MigratorTrait;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn removing_whatsapp_switches_off_live_numbers_and_drops_their_tables() {
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
    super::super::migration::Migrator::up(&db.db, Some(113))
        .await
        .unwrap();
    db.db.execute_unprepared("INSERT INTO relay_workspaces(id,name) VALUES('w1','Mehta');
        INSERT INTO relay_shared_agents(workspace,id,name,role,model) VALUES('w1','nova','Nova','HR','m');
        INSERT INTO relay_communication_endpoints(id,workspace,agent,transport,server,address,simulated) VALUES('wa','w1','nova','whatsapp','twilio','+14155550101',FALSE);
        INSERT INTO relay_communication_endpoints(id,workspace,agent,transport,address) VALUES('sim','w1','nova','whatsapp','sim-number');
        INSERT INTO relay_whatsapp_numbers(number,state,workspace,agent,endpoint,changed) VALUES('+14155550101','assigned','w1','nova','wa',1);").await.unwrap();

    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let state = |id: &'static str| {
        let db = &db;
        async move {
            let row = query(
                &db.db,
                "SELECT enabled,revision FROM relay_communication_endpoints WHERE id=$1",
                vec![id.into()],
            )
            .await
            .unwrap()
            .unwrap();
            (
                row.try_get::<bool>("", "enabled").unwrap(),
                row.try_get::<i64>("", "revision").unwrap(),
            )
        }
    };
    assert_eq!(
        state("wa").await,
        (false, 2),
        "the live number is off, as a new generation"
    );
    assert_eq!(
        state("sim").await,
        (true, 1),
        "local simulations are untouched"
    );
    for table in [
        "relay_whatsapp_numbers",
        "relay_user_mobiles",
        "relay_mobile_link_codes",
    ] {
        let found = query(
            &db.db,
            "SELECT to_regclass($1)::text AS name",
            vec![table.into()],
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            found.try_get::<Option<String>>("", "name").unwrap(),
            None,
            "{table}"
        );
    }
}
