use ayati::{
    platform::{
        ExecutionRun, Platform,
        panels::{PanelDraft, Section},
    },
    storage::Store,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn draft() -> PanelDraft {
    PanelDraft {
        id: uuid::Uuid::new_v4().to_string(),
        expected_revision: 0,
        title: "Stock forecast".into(),
        purpose: "Agent findings from supplied sample data, not a live forecast".into(),
        template: None,
        sections: vec![Section::Metric {
            label: "Sample risk".into(),
            value: "7".into(),
        }],
        sources: vec![],
        publish: false,
        updates_enabled: false,
    }
}

#[tokio::test]
async fn panels_are_reviewed_versioned_scoped_and_refresh_only_with_owner_policy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("business");
    let p = Platform::open(&path, None).unwrap();
    let s = Store::open(&dir.path().join("agent.sqlite")).unwrap();
    s.enqueue("Create a sample forecast panel").unwrap();
    let claim = s.claim_next("worker").unwrap().unwrap();
    let cancel = CancellationToken::new();
    let run = || ExecutionRun {
        agent: "sales",
        store: &s,
        claim: &claim,
        cancel: &cancel,
    };
    let mut input = draft();
    p.write_panel(run(), &input).await.unwrap();
    assert_eq!(
        p.panels(Some("sales")).unwrap()[0]["state"],
        "awaiting_review"
    );
    assert!(
        p.panels(Some("support"))
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(p.activate_panel(&input.id, 2, true).await.is_err());
    p.activate_panel(&input.id, 1, true).await.unwrap();
    input.expected_revision = 1;
    input.sections = vec![Section::Metric {
        label: "Sample risk".into(),
        value: "8".into(),
    }];
    assert!(p.automatic_panel_update("sales", &json!(input)).unwrap());
    assert!(!p.automatic_panel_update("support", &json!(input)).unwrap());
    p.write_panel(run(), &input).await.unwrap();
    assert_eq!(p.panels(Some("sales")).unwrap()[0]["active_revision"], 2);
    assert!(
        p.write_panel(
            ExecutionRun {
                agent: "support",
                store: &s,
                claim: &claim,
                cancel: &cancel
            },
            &input
        )
        .await
        .is_err()
    );
    input.sections = vec![Section::Metric {
        label: "Sample risk".into(),
        value: "9".into(),
    }];
    assert!(p.write_panel(run(), &input).await.is_err());
    input.expected_revision = 2;
    p.set_panel_updates(&input.id, 2, false).await.unwrap();
    assert!(!p.automatic_panel_update("sales", &json!(input)).unwrap());
    p.write_panel(run(), &input).await.unwrap();
    assert_eq!(p.panels(None).unwrap()[0]["active_revision"], 2);
    assert_eq!(p.panels(None).unwrap()[0]["revision"], 3);
    input.expected_revision = 3;
    input.sources = vec!["foreign-execution".into()];
    assert!(p.write_panel(run(), &input).await.is_err());
    p.remove_panel(&input.id, 3).await.unwrap();
    assert!(p.panels(None).unwrap().as_array().unwrap().is_empty());
    input.sources.clear();
    assert!(p.write_panel(run(), &input).await.is_err());
    drop(p);
    let p = Platform::open(&path, None).unwrap();
    assert!(p.panels(None).unwrap().as_array().unwrap().is_empty());
}

#[tokio::test]
async fn revoking_a_source_grant_hides_cached_panel_content_and_foreign_sources_are_denied() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("business");
    let p = Platform::open(&path, None).unwrap();
    let db = rusqlite::Connection::open(path.join("platform.sqlite")).unwrap();
    db.execute_batch("INSERT INTO connections(id,toolkit,label,account_id,auth_config_id,status) VALUES('work','gmail','Work','ca_work','ac_work','ACTIVE'); INSERT INTO grants VALUES('sales','work','GMAIL_FETCH_EMAILS','20261001_00','{}'); INSERT INTO executions(id,agent_id,task_id,connection_id,tool,status) VALUES('receipt','sales','run','work','GMAIL_FETCH_EMAILS','completed');").unwrap();
    let s = Store::open(&dir.path().join("agent.sqlite")).unwrap();
    s.enqueue("Create a sourced panel").unwrap();
    let claim = s.claim_next("worker").unwrap().unwrap();
    let cancel = CancellationToken::new();
    let mut input = draft();
    input.sources = vec!["receipt".into()];
    let run = || ExecutionRun {
        agent: "sales",
        store: &s,
        claim: &claim,
        cancel: &cancel,
    };
    p.write_panel(run(), &input).await.unwrap();
    p.activate_panel(&input.id, 1, true).await.unwrap();
    assert!(p.panels(None).unwrap()[0]["active"].is_object());
    db.execute("DELETE FROM grants WHERE agent_id='sales'", [])
        .unwrap();
    let view = p.panels(None).unwrap();
    assert_eq!(view[0]["state"], "disconnected");
    assert!(view[0]["active"].is_null());
    assert!(view[0]["draft"].is_null());
    assert!(p.write_panel(run(), &input).await.is_err());
}

#[tokio::test]
async fn observed_connector_sources_cannot_be_omitted_from_a_panel_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("business");
    let p = Platform::open(&path, None).unwrap();
    let s = Store::open(&dir.path().join("agent.sqlite")).unwrap();
    s.enqueue("Build a panel").unwrap();
    let claim = s.claim_next("worker").unwrap().unwrap();
    let cancel = CancellationToken::new();
    let db = rusqlite::Connection::open(path.join("platform.sqlite")).unwrap();
    db.execute_batch("INSERT INTO connections(id,toolkit,label,account_id,auth_config_id,status) VALUES('work','gmail','Work','ca_work','ac_work','ACTIVE'); INSERT INTO grants VALUES('sales','work','GMAIL_FETCH_EMAILS','20261001_00','{}');").unwrap();
    db.execute("INSERT INTO executions(id,agent_id,task_id,connection_id,tool,status) VALUES('observed','sales',?1,'work','GMAIL_FETCH_EMAILS','completed')",[&claim.task_id]).unwrap();
    let input = draft();
    p.write_panel(
        ExecutionRun {
            agent: "sales",
            store: &s,
            claim: &claim,
            cancel: &cancel,
        },
        &input,
    )
    .await
    .unwrap();
    assert_eq!(
        p.panels(None).unwrap()[0]["source_executions"],
        json!(["observed"])
    );
    let schedule = ayati::storage::intake::NewSchedule {
        id: uuid::Uuid::new_v4().to_string(),
        content: "Update the panel".into(),
        run_at_ms: 10000,
        interval_seconds: Some(60),
    };
    s.add_schedule(&schedule).unwrap();
    p.bind_panel_schedule("sales", &input.id, &schedule.id)
        .unwrap();
    p.fence_panel_schedules("sales", &s).unwrap();
    assert_eq!(s.schedules().unwrap()[0].state, "paused");
    p.activate_panel(&input.id, 1, true).await.unwrap();
    s.set_schedule_enabled(&schedule.id, true).unwrap();
    p.fence_panel_schedules("sales", &s).unwrap();
    assert_eq!(s.schedules().unwrap()[0].state, "active");
    p.revoke_connection("work").await.unwrap();
    assert_eq!(p.authority_revision("sales").unwrap(), 1);
    p.fence_panel_schedules("sales", &s).unwrap();
    assert_eq!(s.schedules().unwrap()[0].state, "paused");
}

#[tokio::test]
async fn explicit_publication_activates_without_granting_refresh_or_foreign_sources() {
    let dir = tempfile::tempdir().unwrap();
    let p = Platform::open(&dir.path().join("business"), None).unwrap();
    let s = Store::open(&dir.path().join("agent.sqlite")).unwrap();
    s.enqueue("Add a panel to Home").unwrap();
    let claim = s.claim_next("worker").unwrap().unwrap();
    let cancel = CancellationToken::new();
    let run = || ExecutionRun {
        agent: "sales",
        store: &s,
        claim: &claim,
        cancel: &cancel,
    };
    let mut input = draft();
    input.publish = true;
    assert!(!p.automatic_panel_update("sales", &json!(input)).unwrap());
    p.write_panel(run(), &input).await.unwrap();
    let saved = p.panels(None).unwrap();
    assert_eq!(saved[0]["state"], "active");
    assert_eq!(saved[0]["updates_enabled"], false);
    input.expected_revision = 1;
    input.sources = vec!["foreign-source".into()];
    assert!(p.write_panel(run(), &input).await.is_err());
    input.sources.clear();
    input.updates_enabled = true;
    p.write_panel(run(), &input).await.unwrap();
    assert_eq!(p.panels(None).unwrap()[0]["updates_enabled"], true);
    input.updates_enabled = false;
    p.write_panel(run(), &input).await.unwrap();
    assert_eq!(p.panels(None).unwrap()[0]["updates_enabled"], false);
    cancel.cancel();
    assert!(p.write_panel(run(), &input).await.is_err());
}
