//! The drift check against real PostgreSQL and a local Composio stand-in: runs
//! are leased, results saved, and affected agents show in Needs fixing.
use super::super::*;
use super::tests::{second_store, setup};
use crate::identity::connectors::{ConnectorProvider, drift};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_drift_check_saves_results_and_tells_the_people_who_can_fix_affected_agents() {
    let s = setup().await;
    let provider = s.provider.clone();
    let lookup =
        move |_: &str| -> anyhow::Result<Arc<dyn ConnectorProvider>> { Ok(provider.clone()) };
    let today = (now() - now() % 86400) * 1000;
    let causes = |seen: &Value| -> Vec<String> {
        seen["fixing"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["cause"].as_str().unwrap().to_owned())
            .collect()
    };
    // Tonight's run is claimed once: a second controller finds it leased, and
    // once it finished nothing more is due until tomorrow night.
    let run =
        s.db.connector_drift_claim()
            .await
            .unwrap()
            .expect("due on a fresh database");
    assert!(
        second_store(&s.db)
            .await
            .connector_drift_claim()
            .await
            .unwrap()
            .is_none()
    );
    let report = drift::check(&s.db, &run, &lookup).await.unwrap();
    assert_eq!(
        (report["problems"].as_u64(), report["counts"]["ok"].as_u64()),
        (Some(0), Some(3))
    );
    assert!(s.db.connector_drift_claim().await.unwrap().is_none());
    assert!(
        !causes(
            &s.db
                .activity_now(&s.owner, &s.w, today, None)
                .await
                .unwrap()
        )
        .contains(&"drift".into())
    );
    // Composio changed the pinned tools; an operator checks from the terminal.
    s.fake.state.lock().unwrap().changed = true;
    let run = s.db.connector_drift_begin().await.unwrap();
    let report = drift::check(&s.db, &run, &lookup).await.unwrap();
    // Searching lost its query field; drafting and sending need a recipient
    // that is no longer a plain value, so they can't be offered at all.
    assert_eq!(report["problems"], 3, "{report}");
    let actions = &report["accounts"][0]["actions"];
    assert_eq!(
        actions[0],
        json!({"id":"gmail.find_emails","title":"Find emails","status":"changed","fields":["query"],"version":null})
    );
    assert_eq!(
        (actions[1]["status"].as_str(), actions[2]["status"].as_str()),
        (Some("missing"), Some("missing"))
    );
    let saved: i64 = query(&s.db.db, "SELECT count(*) AS n FROM relay_connector_drift WHERE run=$1 AND status IN ('changed','missing')", vec![run.clone().into()])
        .await.unwrap().unwrap().try_get("", "n").unwrap();
    assert_eq!(saved, 3);
    // Needs fixing names the app and the agents that use it, in plain words.
    let seen =
        s.db.activity_now(&s.owner, &s.w, today, None)
            .await
            .unwrap();
    let group = seen["fixing"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["cause"] == "drift")
        .expect("a drift cause")
        .clone();
    assert_eq!(
        group["title"],
        "Gmail changed since it was reviewed · 1 agent affected"
    );
    assert_eq!(
        group["fix"],
        json!({"kind":"settings","label":"Reconnect to review","ref":s.connection})
    );
    assert_eq!(group["agents"][0]["id"], s.agent.as_str());
    // Control counts the last run per provider.
    let health = s.db.connector_health(&["composio"]).await.unwrap();
    assert_eq!(
        health["health"][0]["drift"],
        json!({"ok":0,"missing":2,"changed":1,"new_version":0,"failed":0})
    );
    assert_eq!(health["drift_run"]["via"], "cli");
    // Reconnecting reviews the account again: the old results no longer apply.
    s.sql("UPDATE relay_control_connections SET fingerprint=fingerprint||'-reviewed'")
        .await;
    assert!(
        !causes(
            &s.db
                .activity_now(&s.owner, &s.w, today, None)
                .await
                .unwrap()
        )
        .contains(&"drift".into())
    );
    // A provider that can't answer leaves the action unchecked, which is a problem.
    let broken =
        |_: &str| -> anyhow::Result<Arc<dyn ConnectorProvider>> { anyhow::bail!("not configured") };
    let run = s.db.connector_drift_begin().await.unwrap();
    assert_eq!(
        drift::check(&s.db, &run, &broken).await.unwrap()["counts"]["failed"],
        3
    );
    // An account that needs reconnecting is skipped, and the next night is due again.
    s.sql("UPDATE relay_connector_drift_runs SET started_ms=started_ms-2*86400000")
        .await;
    execute(
        &s.db.db,
        "INSERT INTO relay_connector_reconnects(workspace,connection,since) VALUES($1,$2,1)",
        vec![s.w.clone().into(), s.connection.clone().into()],
    )
    .await
    .unwrap();
    let run =
        s.db.connector_drift_claim()
            .await
            .unwrap()
            .expect("due the next night");
    let report = drift::check(&s.db, &run, &lookup).await.unwrap();
    assert_eq!(
        (
            report["skipped"].as_u64(),
            report["accounts"].as_array().unwrap().len()
        ),
        (Some(1), 0)
    );
}
