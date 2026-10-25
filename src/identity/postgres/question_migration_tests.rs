use super::{tests::*, *};
use sea_orm_migration::MigratorTrait;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn question_upgrade_preserves_a_saved_v22_approval_wait_and_its_checkpoint() {
    let base = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&base).await.unwrap();
    let schema = format!("relay_question_upgrade_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(22)).await.unwrap();
    let owner = user(&db, "question-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let job = super::migration_fixture::legacy_request(
        &db,
        &owner,
        &w,
        &chat,
        &agent,
        "Preserve old approval work",
    )
    .await
    .unwrap()
    .unwrap();
    let claim = uuid::Uuid::new_v4().to_string();
    execute(&db.db,"INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint) VALUES($1,'legacy','orders','{}','reviewed')",vec![w.clone().into()]).await.unwrap();
    execute(&db.db,"INSERT INTO relay_control_actions(id,workspace,agent,chat,job,requester,request_id,connection,operation,arguments,title,effect,rule_revision,connection_revision,approver,state,created,updated,run_cycle,requires_approval) VALUES('old-action',$1,$2,$3,$4,$5,'original','legacy','order.update','{}','Reviewed update','write',1,1,$5,'pending',1,1,0,true)",vec![w.clone().into(),agent.into(),chat.clone().into(),job.reply.clone().into(),owner.id.clone().into()]).await.unwrap();
    execute(&db.db,"UPDATE relay_chat_jobs SET state='waiting',claim=$3,lease_until=0 WHERE workspace=$1 AND reply=$2",vec![w.clone().into(),job.reply.clone().into(),claim.clone().into()]).await.unwrap();
    execute(&db.db,"INSERT INTO relay_task_checkpoints(workspace,reply,cycle,claim,text) VALUES($1,$2,0,$3,'Preserved original checkpoint')",vec![w.clone().into(),job.reply.clone().into(),claim.into()]).await.unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    query(
        &db.db,
        "SELECT relay_resume_waiting_task($1,$2)",
        vec![w.clone().into(), job.reply.clone().into()],
    )
    .await
    .unwrap();
    let saved = query(
        &db.db,
        "SELECT to_jsonb(j) AS data FROM relay_chat_jobs j WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<Value>("", "data")
    .unwrap();
    assert_eq!(saved["state"], "waiting");
    assert_eq!(saved["run_cycle"], 0);
    assert_eq!(saved["payload"], serde_json::to_value(&job).unwrap());
    let history = db.chat_messages(&owner, &w, &chat).await.unwrap();
    assert!(
        history["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == job.reply && m["progress"]["state"] == "approval")
    );
    execute(
        &db.db,
        "UPDATE relay_control_actions SET state='rejected',decision='reject' WHERE id='old-action'",
        vec![],
    )
    .await
    .unwrap();
    let resumed = db.claim_job().await.unwrap().unwrap();
    assert_eq!(resumed.reply, job.reply);
    let data: Value =
        serde_json::from_str(&db.task_continuation(&resumed).await.unwrap().unwrap()).unwrap();
    assert_eq!(data["checkpoint"], "Preserved original checkpoint");
    assert_eq!(data["questions"], json!([]));
    assert_eq!(data["actions"][0]["state"], "rejected");
}
