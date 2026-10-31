use super::tests::{fixture, user, workspace};
use super::*;
use crate::identity::agents::config::{SaveAgent, Settings, instructions};
use sea_orm_migration::MigratorTrait;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_46_joins_old_guidance_into_instructions_and_keeps_short_titles() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_agent_instructions_{}", uuid::Uuid::new_v4().simple());
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
    migration::Migrator::up(&db.db, Some(45)).await.unwrap();
    let long = "Answer customer questions about orders, delivery dates and refunds politely";
    db.db
        .execute_unprepared(&format!(
            "INSERT INTO relay_users(id,provider,email) VALUES('lead','p_lead','lead@test');
             INSERT INTO relay_workspaces(id,name) VALUES('shop','Shop');
             INSERT INTO relay_members(workspace,user_id,role) VALUES('shop','lead','Admin');
             INSERT INTO relay_shared_agents(workspace,id,name,role,model,owner_id,creator_id,revision,settings) VALUES
              ('shop','short','Atlas','Order desk','m','lead','lead',3,'{{\"instructions\":\"Be brief.\",\"escalation\":\"A refund is over 5,000\",\"successful_outcome\":\"The customer knows the date\",\"routine\":\"Check the order first\",\"memory\":true}}'),
              ('shop','long','Nova','{long}','m','lead','lead',1,'{{\"instructions\":\"\",\"escalation\":\"\"}}');"
        ))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();

    let row = |id: &'static str| {
        let db = &db;
        async move {
            query(&db.db, "SELECT role,revision,settings FROM relay_shared_agents WHERE id=$1", vec![id.into()])
                .await
                .unwrap()
                .unwrap()
        }
    };
    let short = row("short").await;
    let settings: Value = short.try_get("", "settings").unwrap();
    assert_eq!(short.try_get::<String>("", "role").unwrap(), "Order desk");
    assert_eq!(short.try_get::<i64>("", "revision").unwrap(), 4);
    assert_eq!(
        settings["instructions"],
        "## Responsibility\nOrder desk\n\n## Instructions\nBe brief.\n\n## Ask a person when\nA refund is over 5,000\n\n## Successful outcome\nThe customer knows the date\n\n## Routine\nCheck the order first"
    );
    assert_eq!(settings["escalation"], "");
    assert!(settings.get("routine").is_none() && settings.get("successful_outcome").is_none());
    assert_eq!(settings["memory"], true);
    let long_row = row("long").await;
    assert_eq!(long_row.try_get::<String>("", "role").unwrap(), "");
    let settings: Value = long_row.try_get("", "settings").unwrap();
    assert_eq!(settings["instructions"], format!("## Responsibility\n{long}"));
    // The agent still reads everything it was told.
    let saved: Settings = serde_json::from_value(settings).unwrap();
    assert!(instructions("Nova", "", Some(&saved)).contains(long));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn saves_record_a_readable_change_and_titles_are_short() {
    let db = fixture().await;
    let owner = user(&db, "changes-owner").await;
    let w = workspace(&db, &owner).await;
    let mut a = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Atlas".into(),
        role: "Order desk".into(),
        model: "fixture".into(),
        audience: "everyone".into(),
        people: None,
        managers: None,
        revision: 0,
        settings: Settings {
            instructions: "Answer order questions.".into(),
            ..Default::default()
        },
    };
    db.save_agent_profile(&owner, &w, &a).await.unwrap();
    a.revision = 1;
    a.audience = "only_me".into();
    a.settings.instructions = "Answer order questions briefly.".into();
    db.save_agent_profile(&owner, &w, &a).await.unwrap();
    let sentences: Vec<String> = rows(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='agent_profile_saved' AND target=$2 ORDER BY id",
        vec![w.clone().into(), a.id.clone().into()],
    )
    .await
    .unwrap()
    .iter()
    .map(|r| r.try_get("", "summary").unwrap())
    .collect();
    assert_eq!(
        sentences,
        [
            "changes-owner@example.test created Atlas",
            "changes-owner@example.test changed Atlas: instructions; who can talk (Everyone → Only me)"
        ]
    );
    a.revision = 2;
    a.role = "x".repeat(81);
    assert!(db.save_agent_profile(&owner, &w, &a).await.is_err());
}
