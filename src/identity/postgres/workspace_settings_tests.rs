use super::super::tests::{fixture, join, message, user, workspace};
use super::preferences::Preferences;
use super::problems::ProblemReport;
use super::workspace::{ModelChoice, WorkspaceSettingsInput};
use super::*;

fn settings(models: &[&str], default: Option<&str>) -> WorkspaceSettingsInput {
    WorkspaceSettingsInput {
        timezone: "Asia/Kolkata".into(),
        colour: Some("teal".into()),
        approval_backup_hours: 6,
        models: models
            .iter()
            .map(|m| ModelChoice {
                provider: "openai".into(),
                model: m.to_string(),
            })
            .collect(),
        default_model: default.map(str::to_owned),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn preferences_follow_the_person_and_set_the_channel_default() {
    let db = fixture().await;
    let owner = user(&db, "pref-owner").await;
    let w = workspace(&db, &owner).await;
    assert_eq!(
        db.preferences(&owner, &w).await.unwrap()["channel_level"],
        "mentions"
    );
    let chosen = Preferences {
        desktop_alerts: false,
        channel_level: "all".into(),
        approval_alerts: false,
    };
    db.save_preferences(&owner, &w, &chosen).await.unwrap();
    // A reload (a fresh read) and the chat bell defaults agree.
    let saved = db.preferences(&owner, &w).await.unwrap();
    assert_eq!(
        (
            saved["desktop_alerts"].clone(),
            saved["channel_level"].clone()
        ),
        (false.into(), "all".into())
    );
    assert_eq!(
        db.chat_notify_levels(&owner, &w).await.unwrap()["defaults"]["channel_level"],
        "all"
    );
    let bad = Preferences {
        channel_level: "loud".into(),
        ..chosen
    };
    assert!(db.save_preferences(&owner, &w, &bad).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn deletion_can_be_undone_for_seven_days_and_then_closes_the_workspace() {
    let db = fixture().await;
    let owner = user(&db, "del-owner").await;
    let member = user(&db, "del-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let name: String = db.workspace_settings(&owner, &w).await.unwrap()["name"]
        .as_str()
        .unwrap()
        .into();
    assert!(
        db.schedule_workspace_deletion(&owner, &w, "wrong name")
            .await
            .is_err()
    );
    assert!(
        db.schedule_workspace_deletion(&member, &w, &name)
            .await
            .is_err(),
        "members cannot delete"
    );
    let at = db
        .schedule_workspace_deletion(&owner, &w, &name)
        .await
        .unwrap()["delete_at_ms"]
        .as_i64()
        .unwrap();
    assert!(at > now() * 1000 + 6 * 24 * 3600 * 1000);
    // Repeating keeps the first date; members see it in their workspace list.
    assert_eq!(
        db.schedule_workspace_deletion(&owner, &w, &name)
            .await
            .unwrap()["delete_at_ms"],
        at
    );
    assert_eq!(
        db.snapshot(&member).await.unwrap()["workspaces"][0]["delete_at_ms"],
        at
    );
    // Undo, then schedule again and let the undo time pass.
    db.cancel_workspace_deletion(&owner, &w).await.unwrap();
    assert_eq!(db.close_deleted_workspaces(10).await.unwrap(), 0);
    assert!(db.workspace_settings(&owner, &w).await.unwrap()["delete_at_ms"].is_null());
    db.schedule_workspace_deletion(&owner, &w, &name)
        .await
        .unwrap();
    let (_, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    execute(
        &db.db,
        "UPDATE relay_workspaces SET delete_at_ms=1 WHERE id=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(db.close_deleted_workspaces(10).await.unwrap(), 1);
    assert_eq!(db.close_deleted_workspaces(10).await.unwrap(), 0);
    // Nobody can open it, it leaves everyone's list, and its agents stop.
    for person in [&owner, &member] {
        assert!(db.preferences(person, &w).await.is_err());
        assert!(
            db.snapshot(person).await.unwrap()["workspaces"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    assert!(
        db.cancel_workspace_deletion(&owner, &w).await.is_err(),
        "no undo after it completes"
    );
    let paused: bool = query(
        &db.db,
        "SELECT (settings->>'paused')::boolean AS p FROM relay_shared_agents WHERE id=$1",
        vec![agent.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "p")
    .unwrap();
    assert!(paused);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_use_only_allowed_models_and_a_removed_model_is_refused() {
    let db = fixture().await;
    let owner = user(&db, "model-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let model: String = query(
        &db.db,
        "SELECT model FROM relay_shared_agents WHERE id=$1",
        vec![agent.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "model")
    .unwrap();
    // Allowing the agent's model and one more: work continues.
    db.save_workspace_settings(
        &owner,
        &w,
        &settings(&[&model, "gpt-5-mini"], Some("gpt-5-mini")),
    )
    .await
    .unwrap();
    db.chat_send(&owner, &w, &c, &message("Still fine", Some(&agent)))
        .await
        .unwrap();
    assert!(
        db.save_workspace_settings(&owner, &w, &settings(&["gpt-5-mini"], Some("unlisted")))
            .await
            .is_err()
    );
    // The admin removes it: new requests are refused with a reason.
    db.save_workspace_settings(&owner, &w, &settings(&["gpt-5-mini"], Some("gpt-5-mini")))
        .await
        .unwrap();
    let refused = db
        .chat_send(&owner, &w, &c, &message("Now refused", Some(&agent)))
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("no longer allowed"),
        "{refused}"
    );
    let (models, default) = db.workspace_models(&owner, &w).await.unwrap();
    assert_eq!(
        (models.as_array().unwrap().len(), default.as_deref()),
        (1, Some("gpt-5-mini"))
    );
    // An empty list allows any model again.
    db.save_workspace_settings(&owner, &w, &settings(&[], None))
        .await
        .unwrap();
    db.chat_send(&owner, &w, &c, &message("Allowed again", Some(&agent)))
        .await
        .unwrap();
    let log = db
        .workspace_activity(&owner, &w, "all", None)
        .await
        .unwrap();
    assert!(log["activity"].as_array().unwrap().iter().any(|e| {
        e["sentence"]
            .as_str()
            .unwrap()
            .contains("allowed any model")
    }));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn members_cannot_use_admin_settings_and_reports_go_to_the_team() {
    let db = fixture().await;
    let owner = user(&db, "audit-owner").await;
    let member = user(&db, "audit-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    assert!(db.workspace_settings(&member, &w).await.is_err());
    assert!(
        db.save_workspace_settings(&member, &w, &settings(&[], None))
            .await
            .is_err()
    );
    assert!(db.cancel_workspace_deletion(&member, &w).await.is_err());
    assert!(
        db.workspace_activity(&member, &w, "all", None)
            .await
            .is_err()
    );
    // The audit log reads as sentences and filters by kind.
    let people = db
        .workspace_activity(&owner, &w, "people", None)
        .await
        .unwrap();
    assert!(
        people["activity"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["kind"] == "people")
    );
    assert!(
        db.workspace_activity(&owner, &w, "everything", None)
            .await
            .is_err()
    );
    // A member's report is stored once, even when the send is retried.
    let report = ProblemReport {
        id: uuid::Uuid::new_v4().to_string(),
        topic: "agent".into(),
        body: "Atlas emailed the wrong customer".into(),
        technical: Some(serde_json::json!({"page": "chat"})),
    };
    db.report_problem(&member, &w, &report).await.unwrap();
    db.report_problem(&member, &w, &report).await.unwrap();
    let changed = ProblemReport {
        body: "Different".into(),
        ..report
    };
    assert!(db.report_problem(&member, &w, &changed).await.is_err());
    let reports = db.problem_reports(10).await.unwrap();
    assert_eq!(reports.iter().filter(|r| r["id"] == changed.id).count(), 1);
    assert_eq!(reports[0]["technical"]["role"], "Member");
}
