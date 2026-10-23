use super::{tests::*, *};

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn assignment_migration_preserves_a_populated_v10_queue() {
    use sea_orm_migration::MigratorTrait;
    let base = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&base).await.unwrap();
    let schema = format!("relay_test_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let old = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&old.db, Some(10)).await.unwrap();
    assert!(old.verify_schema().await.is_err());
    let owner = user(&old, "upgrade-owner").await;
    let w = workspace(&old, &owner).await;
    let c = chat(&old, &owner, &w, vec![], vec!["agent-one".into()]).await;
    let reply = uuid::Uuid::new_v4().to_string();
    let payload = serde_json::json!({"reply":reply,"workspace":w,"chat":c,
        "requester":owner.id,"agent":"agent-one","prompt":"Preserve this old request"});
    // Seed through the old contract: v10 has neither dispatch nor run_seconds.
    execute(&old.db, "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,requester) VALUES($1,$2,$3,'agent-one','agent','','queued',0,$4)", vec![reply.clone().into(),w.clone().into(),c.clone().into(),owner.id.clone().into()]).await.unwrap();
    execute(
        &old.db,
        "INSERT INTO relay_chat_jobs(workspace,reply,payload) VALUES($1,$2,$3)",
        vec![
            w.clone().into(),
            reply.clone().into(),
            payload.clone().into(),
        ],
    )
    .await
    .unwrap();
    old.migrate().await.unwrap();
    old.migrate().await.unwrap(); // Reapplying the migration command is harmless.
    old.verify_schema().await.unwrap();
    let row = query(
        &old.db,
        "SELECT payload,dispatch,run_seconds FROM relay_chat_jobs WHERE reply=$1",
        vec![reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<Value>("", "payload").unwrap(), payload);
    assert_eq!(row.try_get::<String>("", "dispatch").unwrap(), "polling");
    assert_eq!(row.try_get::<i32>("", "run_seconds").unwrap(), 180);
    old.db.execute_unprepared(&format!("GRANT USAGE ON SCHEMA {schema} TO relay_app; GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA {schema} TO relay_app; GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA {schema} TO relay_app; REVOKE UPDATE,DELETE ON {schema}.relay_audit FROM relay_app;")).await.unwrap();
    url.set_username("relay_app").unwrap();
    let runtime = PgStore::connect(url.as_str()).await.unwrap();
    runtime.verify_schema().await.unwrap();
    assert!(runtime.reserve_assignment(6, 2).await.unwrap().is_none());
    let job = runtime.claim_job().await.unwrap().unwrap();
    assert_eq!(job.reply, reply);
    assert!(
        runtime
            .finish_claimed_job(&job, "Migrated result", "complete")
            .await
            .unwrap()
    );
    assert_eq!(
        runtime.chat_messages(&owner, &w, &c).await.unwrap()["messages"][0]["content"],
        "Migrated result"
    );
}

async fn assigned_fixture() -> (PgStore, User, String, String) {
    let mut db = fixture().await;
    db.assigned = true;
    let owner = user(&db, "assignment-owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(
        &db,
        &owner,
        &w,
        vec![],
        vec!["agent-one".into(), "agent-two".into(), "agent-three".into()],
    )
    .await;
    (db, owner, w, c)
}
async fn queue(db: &PgStore, owner: &User, w: &str, c: &str, agent: &str) -> String {
    db.chat_send(owner, w, c, &message("Assignment fixture", Some(agent)))
        .await
        .unwrap()
        .unwrap()
        .reply
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn assigned_jobs_are_not_claimed_by_pollers_and_register_once() {
    let (mut db, owner, w, c) = assigned_fixture().await;
    db.assignment_seconds = 21600;
    let reply = queue(&db, &owner, &w, &c, "agent-one").await;
    assert!(db.claim_job().await.unwrap().is_none());
    let id = db.reserve_assignment(6, 2).await.unwrap().unwrap();
    db.assignment_seconds = 180; // Accepted configuration cannot change on another worker.
    let (a, b) = tokio::join!(
        db.start_assignment(&id, "local:one"),
        db.start_assignment(&id, "local:two")
    );
    let started: Vec<_> = [a, b]
        .into_iter()
        .filter_map(|v| v.ok().flatten())
        .collect();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].job.reply, reply);
    assert_eq!(started[0].run_seconds, 21600);
    assert!(
        db.start_assignment(&uuid::Uuid::new_v4().to_string(), "local:fake")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.finish_claimed_job(&started[0].job, "One result", "complete")
            .await
            .unwrap()
    );
    assert!(
        !db.finish_claimed_job(&started[0].job, "Duplicate", "complete")
            .await
            .unwrap()
    );
    let history = db.chat_messages(&owner, &w, &c).await.unwrap();
    assert_eq!(history["messages"][1]["content"], "One result");
    let row = query(
        &db.db,
        "SELECT state,outcome FROM relay_worker_assignments WHERE id=$1",
        vec![id.into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "finished");
    assert_eq!(row.try_get::<String>("", "outcome").unwrap(), "complete");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn renewed_assignment_survives_expiry_window_and_stale_owner_is_fenced() {
    let (db, owner, w, c) = assigned_fixture().await;
    let reply = queue(&db, &owner, &w, &c, "agent-one").await;
    let old_id = db.reserve_assignment(6, 2).await.unwrap().unwrap();
    let old = db
        .start_assignment(&old_id, "local:old")
        .await
        .unwrap()
        .unwrap();
    execute(&db.db,"UPDATE relay_chat_jobs SET lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+1 WHERE reply=$1",vec![reply.clone().into()]).await.unwrap();
    assert!(db.renew_assignment(&old).await.unwrap());
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(db.chat_job_allowed(&old.job).await);
    assert!(db.reserve_assignment(6, 2).await.unwrap().is_none());
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET lease_until=0 WHERE reply=$1",
        vec![reply.into()],
    )
    .await
    .unwrap();
    assert!(!db.renew_assignment(&old).await.unwrap());
    let new_id = db.reserve_assignment(6, 2).await.unwrap().unwrap();
    let new = db
        .start_assignment(&new_id, "local:new")
        .await
        .unwrap()
        .unwrap();
    assert_ne!(old_id, new_id);
    db.release_assignment(&old_id, "lost", Some("local:old"))
        .await
        .unwrap();
    assert!(db.chat_job_allowed(&new.job).await);
    assert!(
        !db.finish_claimed_job(&old.job, "STALE", "complete")
            .await
            .unwrap()
    );
    assert!(
        db.finish_claimed_job(&new.job, "Replacement result", "complete")
            .await
            .unwrap()
    );
    assert_eq!(
        db.chat_messages(&owner, &w, &c).await.unwrap()["messages"][1]["content"],
        "Replacement result"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn concurrent_launchers_enforce_capacity_and_one_agent_owner() {
    let (db, owner, w, c) = assigned_fixture().await;
    queue(&db, &owner, &w, &c, "agent-one").await;
    queue(&db, &owner, &w, &c, "agent-one").await;
    let (a, b) = tokio::join!(db.reserve_assignment(6, 2), db.reserve_assignment(6, 2));
    let reserved: Vec<_> = [a.unwrap(), b.unwrap()].into_iter().flatten().collect();
    assert_eq!(
        reserved.len(),
        1,
        "One writer per agent even with two controller transactions"
    );
    queue(&db, &owner, &w, &c, "agent-two").await;
    queue(&db, &owner, &w, &c, "agent-three").await;
    let other = db.reserve_assignment(6, 2).await.unwrap().unwrap();
    assert!(db.reserve_assignment(6, 2).await.unwrap().is_none());
    let first = db
        .start_assignment(&reserved[0], "local:first")
        .await
        .unwrap()
        .unwrap();
    db.finish_claimed_job(&first.job, "Done", "complete")
        .await
        .unwrap();
    assert!(db.reserve_assignment(6, 2).await.unwrap().is_some());
    assert!(
        db.start_assignment(&other, "local:other")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn cancellation_is_authorized_durable_and_cannot_be_retried() {
    let (db, owner, w, c) = assigned_fixture().await;
    let member = user(&db, "assignment-member").await;
    join(&db, &owner, &w, &member, "User").await;
    let shared = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec!["agent-one".into()],
    )
    .await;
    let reply = queue(&db, &owner, &w, &shared, "agent-one").await;
    assert!(
        db.cancel_assignment(&member, &w, &shared, &reply)
            .await
            .is_err()
    );
    let id = db.reserve_assignment(6, 2).await.unwrap().unwrap();
    let a = db
        .start_assignment(&id, "local:cancel")
        .await
        .unwrap()
        .unwrap();
    assert!(db.cancel_assignment(&owner, &w, &c, &reply).await.is_err());
    db.cancel_assignment(&owner, &w, &shared, &reply)
        .await
        .unwrap();
    db.cancel_assignment(&owner, &w, &shared, &reply)
        .await
        .unwrap();
    assert!(!db.renew_assignment(&a).await.unwrap());
    assert!(
        !db.finish_claimed_job(&a.job, "MUST NOT PUBLISH", "complete")
            .await
            .unwrap()
    );
    db.release_assignment(&id, "interrupted", Some("local:cancel"))
        .await
        .unwrap();
    assert!(db.reserve_assignment(6, 2).await.unwrap().is_none());
    let history = db.chat_messages(&owner, &w, &shared).await.unwrap();
    assert_eq!(history["messages"][1]["state"], "cancelled");
    assert!(!history.to_string().contains("MUST NOT PUBLISH"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn crashed_final_assignment_has_a_visible_terminal_failure() {
    let (db, owner, w, c) = assigned_fixture().await;
    queue(&db, &owner, &w, &c, "agent-one").await;
    for _ in 0..3 {
        let id = db.reserve_assignment(6, 1).await.unwrap().unwrap();
        db.start_assignment(&id, "local:crash")
            .await
            .unwrap()
            .unwrap();
        db.release_assignment(&id, "lost", Some("local:crash"))
            .await
            .unwrap();
    }
    assert!(db.reserve_assignment(6, 1).await.unwrap().is_none());
    assert_eq!(
        db.chat_messages(&owner, &w, &c).await.unwrap()["messages"][1]["state"],
        "failed"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn assignment_renewal_rechecks_grants_and_workspace_scope() {
    let (db, owner, w, _) = assigned_fixture().await;
    let member = user(&db, "renew-member").await;
    join(&db, &owner, &w, &member, "User").await;
    db.chat_grant(&owner, &w, &member.id, "agent-one", true)
        .await
        .unwrap();
    let c = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec!["agent-one".into()],
    )
    .await;
    queue(&db, &member, &w, &c, "agent-one").await;
    let id = db.reserve_assignment(6, 1).await.unwrap().unwrap();
    let a = db
        .start_assignment(&id, "local:scope")
        .await
        .unwrap()
        .unwrap();
    assert!(db.renew_assignment(&a).await.unwrap());
    let mut forged = a.clone();
    forged.job.workspace = workspace(&db, &member).await;
    assert!(!db.renew_assignment(&forged).await.unwrap());
    db.chat_grant(&owner, &w, &member.id, "agent-one", false)
        .await
        .unwrap();
    assert!(!db.renew_assignment(&a).await.unwrap());
    db.finish_claimed_job(&a.job, "PRIVATE RESULT", "complete")
        .await
        .unwrap();
    assert_eq!(
        db.chat_messages(&owner, &w, &c).await.unwrap()["messages"][1]["state"],
        "cancelled"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn failed_result_save_rolls_back_job_and_attempt_completion() {
    let (db, owner, w, c) = assigned_fixture().await;
    queue(&db, &owner, &w, &c, "agent-one").await;
    let id = db.reserve_assignment(6, 1).await.unwrap().unwrap();
    let assignment = db
        .start_assignment(&id, "local:save")
        .await
        .unwrap()
        .unwrap();
    // PostgreSQL rejects NUL in text after the job/attempt updates in this transaction.
    assert!(
        db.finish_claimed_job(&assignment.job, "bad\0result", "complete")
            .await
            .is_err()
    );
    assert!(db.chat_job_allowed(&assignment.job).await);
    let row = query(
        &db.db,
        "SELECT state FROM relay_worker_assignments WHERE id=$1",
        vec![id.into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "running");
    assert_eq!(
        db.chat_messages(&owner, &w, &c).await.unwrap()["messages"][1]["state"],
        "queued"
    );
    assert!(
        db.finish_claimed_job(&assignment.job, "Durable result", "complete")
            .await
            .unwrap()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn polling_and_assignment_admission_cannot_overlap_the_same_agent() {
    let (mut db, owner, w, c) = assigned_fixture().await;
    db.assigned = false;
    queue(&db, &owner, &w, &c, "agent-one").await;
    db.assigned = true;
    queue(&db, &owner, &w, &c, "agent-one").await;
    let id = db.reserve_assignment(6, 2).await.unwrap().unwrap();
    assert!(db.claim_job().await.unwrap().is_none());
    let a = db
        .start_assignment(&id, "local:assigned")
        .await
        .unwrap()
        .unwrap();
    db.finish_claimed_job(&a.job, "Assigned result", "complete")
        .await
        .unwrap();
    let old_mode = db.claim_job().await.unwrap().unwrap();
    queue(&db, &owner, &w, &c, "agent-one").await;
    assert!(db.reserve_assignment(6, 2).await.unwrap().is_none());
    db.chat_finish(&old_mode, "Polling result", "complete")
        .await
        .unwrap();
    assert!(db.reserve_assignment(6, 2).await.unwrap().is_some());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn losing_launch_cannot_release_a_different_registered_process() {
    let (db, owner, w, c) = assigned_fixture().await;
    queue(&db, &owner, &w, &c, "agent-one").await;
    let id = db.reserve_assignment(6, 1).await.unwrap().unwrap();
    let a = db
        .start_assignment(&id, "local:winner")
        .await
        .unwrap()
        .unwrap();
    assert!(
        !db.record_assignment_resource(&id, "local:loser")
            .await
            .unwrap()
    );
    db.release_assignment(&id, "launch_failed", Some("local:loser"))
        .await
        .unwrap();
    db.release_assignment(&id, "launch_failed", None)
        .await
        .unwrap();
    assert!(db.chat_job_allowed(&a.job).await);
    assert!(db.renew_assignment(&a).await.unwrap());
    assert!(db.reserve_assignment(6, 1).await.unwrap().is_none());
    db.release_assignment(&id, "lost", Some("local:winner"))
        .await
        .unwrap();
    assert!(!db.chat_job_allowed(&a.job).await);
}
