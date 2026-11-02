//! Numbers from the source without a model call: bindings are kept only when they
//! reproduce an update, then the controller runs the next update itself, publishes
//! when nothing meaningful moved, and hands it to the agent when something did.
use super::super::super::super::home_maintenance_tests::{agreement, claim, setup, update};
use super::super::super::super::*;
use crate::identity::{
    chat::model::Job, control::model::ActionRequest, home_panels::maintenance::Refresh,
};
use serde_json::json;

/// One reviewed read in this update, answering `result`.
async fn read(db: &PgStore, job: &Job, connection: &str, result: Value) {
    let action = db
        .control_propose(
            job,
            &ActionRequest {
                connection: connection.into(),
                operation: "order.read".into(),
                arguments: json!({"order":"one"}),
            },
        )
        .await
        .unwrap();
    let dispatch = action.dispatch.unwrap();
    db.control_ready(&job.workspace, &dispatch).await.unwrap();
    db.control_finish(&job.workspace, &dispatch, result, true)
        .await
        .unwrap();
}

fn publish(value: &str, path: Option<&str>) -> Refresh {
    let mut input = json!({"sections":[{"kind":"metric","label":"Order count","value":value}]});
    if let Some(path) = path {
        input["bindings"] = json!([{"target":"value","read":0,"path":path}]);
    }
    serde_json::from_value(input).unwrap()
}

async fn job_row(db: &PgStore, reply: &str) -> Value {
    query(
        &db.db,
        "SELECT to_jsonb(j) AS v FROM relay_chat_jobs j WHERE reply=$1",
        vec![reply.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "v")
    .unwrap()
}

async fn run_row(db: &PgStore, reply: &str) -> Value {
    query(
        &db.db,
        "SELECT to_jsonb(r) AS v FROM relay_home_update_runs r WHERE reply=$1",
        vec![reply.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "v")
    .unwrap()
}

async fn saved_bindings(db: &PgStore, panel: &str) -> Value {
    query(
        &db.db,
        "SELECT bindings FROM relay_home_maintenance WHERE panel=$1",
        vec![panel.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<Option<Value>>("", "bindings")
    .unwrap()
    .unwrap_or(Value::Null)
}

async fn update_now(db: &PgStore, owner: &User, w: &str, panel: &str) -> String {
    let revision = db.home_panels(owner, w).await.unwrap()["panels"][0]["maintenance"]["revision"]
        .as_i64()
        .unwrap();
    db.update_home_panel(owner, w, panel, &update(revision))
        .await
        .unwrap()["reply"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_controller_refreshes_numbers_itself_once_bindings_reproduce_an_update() {
    let (db, owner, w, _, read_connection, _, p) = setup().await;
    db.save_home_agreement(&owner, &w, &p.id, &agreement(0, None))
        .await
        .unwrap();

    // Bindings that don't reproduce what the agent published are not kept.
    let first = claim(&db, &w, &update_now(&db, &owner, &w, &p.id).await).await;
    read(&db, &first, &read_connection, json!({"count": 2})).await;
    let saved = db
        .refresh_home_panel(&first, &publish("2", Some("total")))
        .await
        .unwrap();
    assert_eq!(saved["bindings"]["saved"], false);
    assert!(
        saved["bindings"]["message"]
            .as_str()
            .unwrap()
            .contains("Nothing at total")
    );
    assert!(saved_bindings(&db, &p.id).await.is_null());
    db.chat_finish(&first, "Updated", "complete").await.unwrap();

    // Bindings that do are kept, and the agent's context shows them.
    let second = claim(&db, &w, &update_now(&db, &owner, &w, &p.id).await).await;
    read(
        &db,
        &second,
        &read_connection,
        json!({"content":[{"type":"text","text":"{\"count\":2}"}]}),
    )
    .await;
    assert!(db.home_update_context(&second).await.unwrap()["bindings"].is_null());
    let saved = db
        .refresh_home_panel(&second, &publish("2", Some("count")))
        .await
        .unwrap();
    assert_eq!(saved["bindings"]["saved"], true, "{saved}");
    assert_eq!(saved_bindings(&db, &p.id).await[0]["path"], "count");
    db.chat_finish(&second, "Updated", "complete")
        .await
        .unwrap();

    // The next update is the controller's: claimed before any executor sees it.
    let third = update_now(&db, &owner, &w, &p.id).await;
    let row = job_row(&db, &third).await;
    assert_eq!(row["state"], "running");
    assert!(row["claim"].as_str().unwrap().starts_with("direct-"));
    let picked = db.direct_pick().await.unwrap();
    assert_eq!(picked.len(), 1);
    assert!(db.direct_pick().await.unwrap().is_empty(), "taken once");
    let job = picked[0].clone();
    let plan = db.direct_plan(&job).await.unwrap();
    assert_eq!(plan.sections[0]["value"], "2");
    read(
        &db,
        &job,
        &read_connection,
        json!({"structuredContent":{"count":2.1}}),
    )
    .await;
    let results = db.direct_results(&job, &plan.scopes).await.unwrap();
    let section =
        crate::panels::bindings::apply(&plan.sections[0], &plan.bindings, &results).unwrap();
    assert_eq!(section["value"], "2.10");
    assert!(!crate::panels::bindings::meaningful(
        &plan.sections[0],
        &section,
        &plan.bindings,
        0.10
    ));
    db.refresh_home_panel(
        &job,
        &serde_json::from_value(json!({"sections":[section]})).unwrap(),
    )
    .await
    .unwrap();
    db.direct_done(&job).await.unwrap();
    db.chat_finish(&job, "Numbers refreshed from the source.", "complete")
        .await
        .unwrap();
    assert_eq!(run_row(&db, &third).await["direct"], true);
    let panel = db.home_panels(&owner, &w).await.unwrap()["panels"][0].clone();
    assert_eq!(
        panel["active"]["definition"]["sections"][0]["value"],
        "2.10"
    );

    // A meaningful move goes to the agent, with the reason in its context.
    let fourth = update_now(&db, &owner, &w, &p.id).await;
    let job = db.direct_pick().await.unwrap().remove(0);
    read(&db, &job, &read_connection, json!({"count": 5})).await;
    assert!(
        db.direct_release(
            &job,
            "Something meaningful changed: the agent writes what changed"
        )
        .await
        .unwrap()
    );
    let row = job_row(&db, &fourth).await;
    assert_eq!(
        (row["state"].as_str(), row["claim"].as_str()),
        (Some("queued"), None)
    );
    let agent = claim(&db, &w, &fourth).await;
    let context = db.home_update_context(&agent).await.unwrap();
    assert!(
        context["handed_back"]
            .as_str()
            .unwrap()
            .starts_with("Something meaningful")
    );
    assert_eq!(run_row(&db, &fourth).await["direct"], false);

    // Changing the agreement forgets the bindings.
    db.chat_finish(&agent, "Handled", "complete").await.unwrap();
    let revision =
        db.home_panels(&owner, &w).await.unwrap()["panels"][0]["maintenance"]["revision"]
            .as_i64()
            .unwrap();
    db.save_home_agreement(&owner, &w, &p.id, &agreement(revision, None))
        .await
        .unwrap();
    assert!(saved_bindings(&db, &p.id).await.is_null());
    let fifth = update_now(&db, &owner, &w, &p.id).await;
    assert_eq!(
        job_row(&db, &fifth).await["state"],
        "queued",
        "without bindings the agent updates"
    );
}
