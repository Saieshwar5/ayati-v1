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
        kind: "added".into(),
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

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_changed_request_starts_work_with_what_changed() {
    let mut db = fixture().await;
    db.pooled = true;
    let owner = user(&db, "changes-owner").await;
    let mina = user(&db, "changes-mina").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &mina, "Member").await;
    let (_, agent) = executor_slot_tests::agent(&db, &owner, &w).await;
    let id = uuid::Uuid::new_v4().to_string();
    db.built_app_create(
        &owner,
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
    db.built_app_publish(
        &owner,
        &w,
        &id,
        &AppRelease {
            note: String::new(),
            revision: 1,
        },
    )
    .await
    .unwrap();
    db.built_app_roles_set(
        &owner,
        &w,
        &id,
        &RoleChange {
            role: "approver".into(),
            people: vec![owner.id.clone()],
        },
    )
    .await
    .unwrap();
    let changed = AppWatch {
        collection: "requests".into(),
        kind: "changed".into(),
        agent: Some(agent.clone()),
        instructions: "When a request is decided, record it in payroll.".into(),
    };
    let bad = AppWatch {
        kind: "deleted".into(),
        ..AppWatch {
            collection: "requests".into(),
            kind: String::new(),
            agent: Some(agent.clone()),
            instructions: "x".into(),
        }
    };
    assert!(db.built_app_watch_set(&owner, &w, &id, &bad).await.is_err());
    db.built_app_watch_set(&owner, &w, &id, &changed)
        .await
        .unwrap();
    // Adding isn't watched; deciding it is, with only what changed.
    let asked = db
        .built_app_data(
            &mina,
            &w,
            &id,
            &create(
                json!({"start":"2026-11-10","days":2,"status":"waiting"}),
                false,
            ),
        )
        .await
        .unwrap();
    let record = asked["id"].as_str().unwrap().to_owned();
    assert_eq!(db.dispatch_app_events().await.unwrap(), 0);
    let decide = DataRequest {
        preview: false,
        op: "update".into(),
        collection: "requests".into(),
        record: Some(record.clone()),
        data: Some(json!({"status":"approved"})),
        revision: Some(1),
        mine: false,
    };
    db.built_app_data(&owner, &w, &id, &decide).await.unwrap();
    assert_eq!(db.dispatch_app_events().await.unwrap(), 1);
    let job = claim(&db, &record).await;
    assert!(job.prompt.contains("Changed in the app"), "{}", job.prompt);
    assert!(
        job.prompt.contains("changed by changes-owner") && job.prompt.contains("status: approved"),
        "{}",
        job.prompt
    );
    assert!(!job.prompt.contains("days: 2"), "only what changed");
    let listed = db.built_app_watchers(&owner, &w, &id).await.unwrap();
    assert_eq!(listed["watchers"][0]["kind"], "changed");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_agents_app_triggers_show_only_in_apps_the_viewer_may_open() {
    let db = fixture().await;
    let owner = user(&db, "app-triggers-owner").await;
    let ravi = user(&db, "app-triggers-ravi").await;
    let mina = user(&db, "app-triggers-mina").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &ravi, "Member").await;
    join(&db, &owner, &w, &mina, "Member").await;
    let (_, agent) = executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &ravi.id, &agent, true)
        .await
        .unwrap();
    for (name, audience) in [("Leave", "everyone"), ("Salaries", "selected")] {
        let id = uuid::Uuid::new_v4().to_string();
        let app = NewApp {
            id: id.clone(),
            name: name.into(),
            description: String::new(),
            audience: audience.into(),
            people: vec![],
            template: "leave".into(),
        };
        db.built_app_create(&ravi, &w, &app).await.unwrap();
        let release = AppRelease {
            note: String::new(),
            revision: 1,
        };
        db.built_app_publish(&ravi, &w, &id, &release)
            .await
            .unwrap();
        let watch = AppWatch {
            collection: "requests".into(),
            kind: "added".into(),
            agent: Some(agent.clone()),
            instructions: "Tell me who asked.".into(),
        };
        db.built_app_watch_set(&ravi, &w, &id, &watch)
            .await
            .unwrap();
    }
    let seen = |who: &crate::identity::user::User| {
        let (db, w, agent, who) = (&db, w.clone(), agent.clone(), who.clone());
        async move {
            let tx = db.read_tx(&w).await.unwrap();
            let list = super::agent_app_triggers(&tx, &who, &w, &agent)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            list.iter()
                .map(|t| {
                    (
                        t["app_name"].as_str().unwrap().to_owned(),
                        t["manages"] == true,
                    )
                })
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(
        seen(&ravi).await,
        vec![("Leave".into(), true), ("Salaries".into(), true)],
        "its builder sees both and may change them"
    );
    assert_eq!(
        seen(&mina).await,
        vec![("Leave".into(), false)],
        "an app she can't open stays hidden"
    );
    // The agent's managers see them with its event triggers.
    let listed = db.event_triggers(&owner, &w, &agent, &[]).await.unwrap();
    assert_eq!(listed["app_triggers"].as_array().unwrap().len(), 2);
    assert_eq!(listed["app_triggers"][0]["collection"], "requests");
    assert!(listed["app_triggers"][0]["by"].is_string());
}
