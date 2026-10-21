//! These tests deliberately require a real, disposable PostgreSQL database.
use super::*;
use crate::identity::{
    chat::store::{NewChat, NewMessage},
    store::Store,
};
use sea_orm::ConnectionTrait;

pub(super) async fn fixture() -> PgStore {
    let url = std::env::var("RELAY_TEST_DATABASE_URL")
        .expect("Set RELAY_TEST_DATABASE_URL to a disposable PostgreSQL database");
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
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    db.db.execute_unprepared(&format!("GRANT USAGE ON SCHEMA {schema} TO relay_app; GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA {schema} TO relay_app; GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA {schema} TO relay_app; REVOKE UPDATE,DELETE ON {schema}.relay_audit FROM relay_app;")).await.unwrap();
    url.set_username("relay_app").unwrap();
    let runtime = PgStore::connect(url.as_str()).await.unwrap();
    runtime.verify_schema().await.unwrap();
    runtime
}
pub(super) async fn user(db: &PgStore, name: &str) -> User {
    db.login(
        name,
        &format!("{name}@example.test"),
        name,
        "test-provider-session",
    )
    .await
    .unwrap()
}
pub(super) async fn workspace(db: &PgStore, owner: &User) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    db.create(owner, "Test business", &id).await.unwrap()
}
async fn join(db: &PgStore, owner: &User, w: &str, member: &User, role: &str) {
    let invite = db
        .reserve_invite(owner, w, &member.email, role)
        .await
        .unwrap();
    db.delivered(&invite, Some("fixture-delivery"))
        .await
        .unwrap();
    db.accept(member, &invite).await.unwrap();
}
pub(super) async fn chat(
    db: &PgStore,
    owner: &User,
    w: &str,
    users: Vec<String>,
    agents: Vec<String>,
) -> String {
    db.chat_create(
        owner,
        w,
        &NewChat {
            kind: "group".into(),
            description: String::new(),
            id: uuid::Uuid::new_v4().to_string(),
            name: "Operations".into(),
            users,
            agents,
        },
    )
    .await
    .unwrap()
}
pub(super) fn message(text: &str, agent: Option<&str>) -> NewMessage {
    NewMessage {
        thread: None,
        id: uuid::Uuid::new_v4().to_string(),
        content: text.into(),
        agent: agent.map(str::to_owned),
    }
}

#[tokio::test]
#[ignore = "requires local PostgreSQL; run with RELAY_TEST_DATABASE_URL"]
async fn accounts_membership_and_concurrent_idempotency() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let stranger = user(&db, "stranger").await;
    let id = uuid::Uuid::new_v4().to_string();
    let (a, b) = tokio::join!(
        db.create(&owner, "Business", &id),
        db.create(&owner, "Business", &id)
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert!(db.create(&stranger, "Business", &id).await.is_err());
    assert!(db.create(&owner, "Changed payload", &id).await.is_err());
    assert!(db.people(&stranger, &id).await.is_err());
    assert!(db.role(&id, &stranger.id).await.is_err());
    assert!(
        db.snapshot(&stranger).await.unwrap()["workspaces"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.people(&owner, &id).await.unwrap()["members"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(db.session("owner").await.unwrap().unwrap().0.id, owner.id);
    db.logout("owner").await.unwrap();
    assert!(db.session("owner").await.unwrap().is_none());
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn invitation_replay_delivery_and_role_boundaries() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let admin = user(&db, "admin").await;
    let member = user(&db, "member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &admin, "Admin").await;
    assert!(
        db.reserve_invite(&admin, &w, &member.email, "Admin")
            .await
            .is_err()
    );
    let invite = db
        .reserve_invite(&admin, &w, &member.email, "Member")
        .await
        .unwrap();
    assert!(
        db.reserve_invite(&admin, &w, &member.email, "Member")
            .await
            .is_err()
    );
    assert!(db.accept(&member, &invite).await.is_err());
    db.delivered(&invite, Some("delivered")).await.unwrap();
    assert!(db.accept(&owner, &invite).await.is_err());
    db.accept(&member, &invite).await.unwrap();
    assert!(db.change(&admin, &w, &owner.id, None).await.is_err());
    assert!(db.change(&member, &w, &admin.id, None).await.is_err());
    db.change(&admin, &w, &member.id, None).await.unwrap();
    assert!(db.accept(&member, &invite).await.is_err());
    let pending = db
        .reserve_invite(&admin, &w, &member.email, "Member")
        .await
        .unwrap();
    db.delivered(&pending, Some("delivered")).await.unwrap();
    db.change(&owner, &w, &admin.id, Some("Member"))
        .await
        .unwrap();
    assert!(db.accept(&member, &pending).await.is_err());
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn conversations_isolate_workspaces_private_history_and_duplicates() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let member = user(&db, "member").await;
    let w = workspace(&db, &owner).await;
    let other = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let shared = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec!["agent-one".into()],
    )
    .await;
    let private = chat(&db, &owner, &w, vec![], vec![]).await;
    db.chat_send(&owner, &w, &private, &message("PRIVATE SALARY", None))
        .await
        .unwrap();
    assert!(db.chat_messages(&member, &w, &private).await.is_err());
    assert!(db.chat_messages(&owner, &other, &shared).await.is_err());
    assert!(
        db.chat_send(&member, &w, &shared, &message("hello", Some("agent-one")))
            .await
            .is_err()
    );
    db.chat_grant(&owner, &w, &member.id, "agent-one", true)
        .await
        .unwrap();
    let input = message("Prepare quotation", Some("agent-one"));
    let (a, b) = tokio::join!(
        db.chat_send(&member, &w, &shared, &input),
        db.chat_send(&member, &w, &shared, &input)
    );
    let jobs = [a.unwrap(), b.unwrap()];
    assert_eq!(jobs.iter().filter(|j| j.is_some()).count(), 1);
    assert!(
        !jobs
            .iter()
            .flatten()
            .next()
            .unwrap()
            .prompt
            .contains("PRIVATE SALARY")
    );
    assert_eq!(
        db.chat_messages(&owner, &w, &shared).await.unwrap()["messages"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let invalid = execute(
        &db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![other.into(), shared.into(), owner.id.into()],
    )
    .await;
    assert!(
        invalid.is_err(),
        "Composite workspace/conversation foreign key must reject cross-tenant links"
    );
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn durable_claims_recover_and_fence_stale_workers() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let w = workspace(&db, &owner).await;
    let conversation = chat(&db, &owner, &w, vec![], vec!["agent-one".into()]).await;
    db.chat_send(
        &owner,
        &w,
        &conversation,
        &message("hello", Some("agent-one")),
    )
    .await
    .unwrap();
    let (a, b) = tokio::join!(db.claim_job(), db.claim_job());
    let claims = [a.unwrap(), b.unwrap()];
    assert_eq!(claims.iter().flatten().count(), 1);
    let first = claims.into_iter().flatten().next().unwrap();
    assert!(db.chat_job_allowed(&first).await);
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET lease_until=0 WHERE reply=$1",
        vec![first.reply.clone().into()],
    )
    .await
    .unwrap();
    // A fresh pool has no in-memory knowledge of the previous worker.
    let schema: String = query(&db.db, "SELECT current_schema() AS name", vec![])
        .await
        .unwrap()
        .unwrap()
        .try_get("", "name")
        .unwrap();
    let mut url = reqwest::Url::parse(&std::env::var("RELAY_TEST_DATABASE_URL").unwrap()).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let replacement = PgStore::connect(url.as_str()).await.unwrap();
    let recovered = replacement.claim_job().await.unwrap().unwrap();
    assert_ne!(first.claim, recovered.claim);
    assert!(!db.chat_job_allowed(&first).await);
    db.chat_finish(&first, "STALE RESULT", "complete")
        .await
        .unwrap();
    db.chat_finish(&recovered, "Recovered result", "complete")
        .await
        .unwrap();
    let messages = db.chat_messages(&owner, &w, &conversation).await.unwrap();
    assert_eq!(messages["messages"][1]["content"], "Recovered result");
    assert!(db.claim_job().await.unwrap().is_none());
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn revocation_cancels_pending_results_and_resource_sessions() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let member = user(&db, "member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let c = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec!["agent-one".into()],
    )
    .await;
    db.chat_grant(&owner, &w, &member.id, "agent-one", true)
        .await
        .unwrap();
    db.resource_grant(&owner, &w, &member.id, "app", "app-one", Some("read"))
        .await
        .unwrap();
    db.app_session_save(&member, &w, "app-one", "hash", "doc")
        .await
        .unwrap();
    assert!(
        db.app_session_bound(&member, &w, "app-one", "hash", false)
            .await
            .unwrap()
    );
    db.chat_send(&member, &w, &c, &message("hello", Some("agent-one")))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.change(&owner, &w, &member.id, None).await.unwrap();
    assert!(!db.chat_job_allowed(&job).await);
    db.chat_finish(&job, "MUST NOT PUBLISH", "complete")
        .await
        .unwrap();
    assert_eq!(
        db.chat_messages(&owner, &w, &c).await.unwrap()["messages"][1]["state"],
        "cancelled"
    );
    assert!(db.chat_messages(&member, &w, &c).await.is_err());
    assert!(
        db.app_session_bound(&member, &w, "app-one", "hash", false)
            .await
            .is_err()
    );
    join(&db, &owner, &w, &member, "Member").await;
    assert!(
        db.resource_allowed(&member, &w, "app", "app-one")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !db.app_session_bound(&member, &w, "app-one", "hash", false)
            .await
            .unwrap()
    );
    assert!(db.chat_messages(&member, &w, &c).await.is_err());
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn sqlite_import_preserves_ids_and_history_without_sessions() {
    let db = fixture().await;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("identity.sqlite");
    let old = Store::open(&path).unwrap();
    let owner = old
        .login("owner", "owner@example.test", "secret-session-hash", "sid")
        .unwrap();
    let w = uuid::Uuid::new_v4().to_string();
    old.create(&owner, "Original business", &w).unwrap();
    let c = old
        .chat_create(
            &owner,
            &w,
            &NewChat {
                kind: "group".into(),
                description: String::new(),
                id: uuid::Uuid::new_v4().to_string(),
                name: "Original chat".into(),
                users: vec![],
                agents: vec![],
            },
        )
        .unwrap();
    old.update_profile(&owner, "Imported person").unwrap();
    old.chat_send(&owner, &w, &c, &message("Keep this history", None))
        .unwrap();
    drop(old);
    let counts = db.import_sqlite(&path).await.unwrap();
    assert_eq!(counts["users"], 1);
    let imported = db
        .login("owner", "owner@example.test", "new-hash", "sid")
        .await
        .unwrap();
    assert_eq!(imported.display_name.as_deref(), Some("Imported person"));
    assert_eq!(db.role(&w, &owner.id).await.unwrap(), "Owner");
    assert_eq!(
        db.chat_messages(&owner, &w, &c).await.unwrap()["messages"][0]["content"],
        "Keep this history"
    );
    assert!(db.session("secret-session-hash").await.unwrap().is_none());
    assert!(db.import_sqlite(&path).await.is_err());
    let source =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    assert_eq!(
        source
            .query_row("SELECT content FROM chat_messages", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "Keep this history"
    );
}
#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn invalid_import_rolls_back_all_target_records() {
    let db = fixture().await;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("identity.sqlite");
    let old = Store::open(&path).unwrap();
    old.login("owner", "owner@example.test", "hash", "sid")
        .unwrap();
    old.0
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO chat_people VALUES('missing-chat','missing-user')",
            [],
        )
        .unwrap();
    assert!(db.import_sqlite(&path).await.is_err());
    let count: i64 = query(&db.db, "SELECT count(*) AS total FROM relay_users", vec![])
        .await
        .unwrap()
        .unwrap()
        .try_get("", "total")
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn runtime_role_cannot_change_schema_or_delete_audit() {
    let db = fixture().await;
    assert!(
        db.db
            .execute_unprepared("CREATE TABLE unauthorized(id TEXT)")
            .await
            .is_err()
    );
    assert!(
        db.db
            .execute_unprepared("DELETE FROM relay_audit")
            .await
            .is_err()
    );
    let elevated:bool=query(&db.db,"SELECT rolsuper OR rolcreatedb OR rolcreaterole AS elevated FROM pg_roles WHERE rolname=current_user",vec![]).await.unwrap().unwrap().try_get("","elevated").unwrap();
    assert!(!elevated);
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn concurrent_messages_are_saved_once_in_each_workspace() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let a = workspace(&db, &owner).await;
    let b = workspace(&db, &owner).await;
    let ca = chat(&db, &owner, &a, vec![], vec![]).await;
    let cb = chat(&db, &owner, &b, vec![], vec![]).await;
    let inputs = (0..32)
        .map(|i| message(&format!("Message {i}"), None))
        .collect::<Vec<_>>();
    let results = futures_util::future::join_all(inputs.iter().enumerate().map(|(i, input)| {
        let (w, c) = if i % 2 == 0 { (&a, &ca) } else { (&b, &cb) };
        db.chat_send(&owner, w, c, input)
    }))
    .await;
    assert!(results.iter().all(|r| matches!(r, Ok(None))));
    for (w, c) in [(&a, &ca), (&b, &cb)] {
        let data = db.chat_messages(&owner, w, c).await.unwrap();
        assert_eq!(data["messages"].as_array().unwrap().len(), 16);
    }
}

#[path = "conversation_tests.rs"]
mod conversations;
