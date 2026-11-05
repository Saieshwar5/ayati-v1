//! App events: a new live record starts work for the collection's watcher, as them and
//! in their conversation; preview records and the agent's own records don't.
use super::super::super::executor_slot_tests;
use super::super::super::tests::{fixture, join, user, workspace};
use super::super::{AgentRule, AppRelease, DataRequest, NewApp, RoleChange};
use super::AppWatch;
use crate::identity::control::broker::invoke;
use crate::identity::postgres::{PgStore, execute, query, rows};
use serde_json::json;

async fn claim(db: &PgStore, record: &str) -> crate::identity::chat::model::Job {
    let row = query(&db.db, "SELECT j.payload FROM relay_chat_jobs j JOIN relay_built_app_events e ON e.workspace=j.workspace AND e.reply=j.reply WHERE e.record=$1", vec![record.into()]).await.unwrap().unwrap();
    let mut job: crate::identity::chat::model::Job =
        serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap();
    job.claim = Some(uuid::Uuid::new_v4().to_string());
    execute(&db.db, "UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1", vec![job.reply.clone().into(), job.claim.clone().into()]).await.unwrap();
    job
}
fn create(data: serde_json::Value, preview: bool) -> DataRequest {
    DataRequest {
        preview,
        op: "create".into(),
        collection: "requests".into(),
        record: None,
        data: Some(data),
        revision: None,
        mine: false,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn new_leave_requests_start_work_for_the_approver_who_asked() {
    let mut db = fixture().await;
    db.pooled = true;
    let owner = user(&db, "events-owner").await;
    let ravi = user(&db, "events-ravi").await;
    let mina = user(&db, "events-mina").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &ravi, "Member").await;
    join(&db, &owner, &w, &mina, "Member").await;
    let (_, agent) = executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &ravi.id, &agent, true)
        .await
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    db.built_app_create(
        &ravi,
        &w,
        &NewApp {
            id: id.clone(),
            name: "Leave".into(),
            description: String::new(),
            audience: "everyone".into(),
            people: vec![],
            template: "leave".into(),
        },
    )
    .await
    .unwrap();
    let watch = |agent: Option<&str>, collection: &str| AppWatch {
        collection: collection.into(),
        agent: agent.map(str::to_owned),
        instructions: "Tell me who asked and for how long, and open a task for me to decide."
            .into(),
    };
    assert!(
        db.built_app_watch_set(&ravi, &w, &id, &watch(Some(&agent), "requests"))
            .await
            .unwrap_err()
            .to_string()
            .contains("Publish the app first")
    );
    db.built_app_publish(
        &ravi,
        &w,
        &id,
        &AppRelease {
            note: String::new(),
            revision: 1,
        },
    )
    .await
    .unwrap();
    assert!(
        db.built_app_watch_set(&mina, &w, &id, &watch(Some(&agent), "requests"))
            .await
            .is_err(),
        "only its builder or an admin"
    );
    assert!(
        db.built_app_watch_set(&ravi, &w, &id, &watch(Some(&agent), "salaries"))
            .await
            .is_err()
    );
    let set = db
        .built_app_watch_set(&ravi, &w, &id, &watch(Some(&agent), "requests"))
        .await
        .unwrap();
    let chat = set["chat"].as_str().unwrap().to_owned();
    let listed = db.built_app_watchers(&mina, &w, &id).await.unwrap();
    assert_eq!(listed["collections"], json!(["requests"]));
    assert!(
        listed["watchers"][0]["chat"].is_null(),
        "only the watcher gets the conversation link"
    );
    // Ravi approves; the agent will act as him.
    db.built_app_roles_set(
        &ravi,
        &w,
        &id,
        &RoleChange {
            role: "approver".into(),
            people: vec![ravi.id.clone()],
        },
    )
    .await
    .unwrap();
    db.built_app_agent_rule(
        &ravi,
        &w,
        &id,
        &AgentRule {
            action: "my_leave".into(),
            mode: "allow".into(),
        },
    )
    .await
    .unwrap();
    db.built_app_agent_rule(
        &ravi,
        &w,
        &id,
        &AgentRule {
            action: "file_leave".into(),
            mode: "allow".into(),
        },
    )
    .await
    .unwrap();
    // Mina asks for leave in the app; a preview record doesn't count.
    let asked = db
        .built_app_data(
            &mina,
            &w,
            &id,
            &create(
                json!({"start":"2026-11-10","days":2,"reason":"Family","status":"waiting"}),
                false,
            ),
        )
        .await
        .unwrap();
    db.built_app_data(&ravi, &w, &id, &create(json!({"days":1}), true))
        .await
        .unwrap();
    let record = asked["id"].as_str().unwrap().to_owned();
    assert_eq!(db.dispatch_app_events().await.unwrap(), 1);
    assert_eq!(db.dispatch_app_events().await.unwrap(), 0);
    let job = claim(&db, &record).await;
    assert_eq!(job.requester, ravi.id);
    assert_eq!(job.chat, chat);
    assert!(
        job.prompt.contains("added by events-mina"),
        "{}",
        job.prompt
    );
    assert!(job.prompt.contains("days: 2") && job.prompt.contains(&record));
    // As Ravi (an approver) it reads every request; what it adds itself isn't an event.
    let seen = invoke(
        &db,
        &job,
        "run_app_action",
        json!({"app":id,"action":"my_leave"}),
    )
    .await
    .unwrap();
    assert_eq!(seen["result"]["records"].as_array().unwrap().len(), 1);
    invoke(
        &db,
        &job,
        "run_app_action",
        json!({"app":id,"action":"file_leave","data":{"start":"2026-12-01","days":1}}),
    )
    .await
    .unwrap();
    let queued: i64 = rows(
        &db.db,
        "SELECT count(*) AS n FROM relay_built_app_events WHERE app=$1",
        vec![id.clone().into()],
    )
    .await
    .unwrap()[0]
        .try_get("", "n")
        .unwrap();
    assert_eq!(queued, 1, "the agent's own record started nothing");
    // A second request waits until the first is handled.
    let second = db
        .built_app_data(&mina, &w, &id, &create(json!({"days":3}), false))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(db.dispatch_app_events().await.unwrap(), 0);
    db.chat_finish(&job, "Mina asked for 2 days from 10 November.", "complete")
        .await
        .unwrap();
    assert_eq!(db.dispatch_app_events().await.unwrap(), 1);
    let next = claim(&db, &second).await;
    db.chat_finish(&next, "Mina asked for 3 days.", "complete")
        .await
        .unwrap();
    // Stopping skips what still waits.
    let third = db
        .built_app_data(&mina, &w, &id, &create(json!({"days":4}), false))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    db.built_app_watch_set(&ravi, &w, &id, &watch(None, "requests"))
        .await
        .unwrap();
    assert_eq!(db.dispatch_app_events().await.unwrap(), 0);
    let state: String = rows(
        &db.db,
        "SELECT state FROM relay_built_app_events WHERE record=$1",
        vec![third.into()],
    )
    .await
    .unwrap()[0]
        .try_get("", "state")
        .unwrap();
    assert_eq!(state, "skipped");
    let said: Vec<String> = rows(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND summary IS NOT NULL",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.try_get("", "summary").unwrap())
    .collect();
    let said = said.join("\n");
    assert!(
        said.contains("to act on new requests in \"Leave\""),
        "{said}"
    );
    assert!(
        said.contains("Stopped agents acting on new requests in \"Leave\""),
        "{said}"
    );
}
