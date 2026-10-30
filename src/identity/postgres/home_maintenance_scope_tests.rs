use super::{home_maintenance_tests::*, tests::*, *};
use crate::identity::{
    control::model::Rule,
    home_panels::{maintenance::Agreement, model::Review},
};
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn home_maintenance_member_owns_updates_without_agent_management_authority() {
    let (mut db, owner, w, original, read, _, mut p) = setup().await;
    let member = user(&db, "personal-panel-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    db.chat_grant(&owner, &w, &member.id, &original.agent, true)
        .await
        .unwrap();
    let c = chat(
        &db,
        &member,
        &w,
        vec![owner.id.clone()],
        vec![original.agent.clone()],
    )
    .await;
    db.control_rule(
        &owner,
        &w,
        &original.agent,
        &Rule {
            connection: read.clone(),
            operation: "order.read".into(),
            mode: "allow".into(),
            revision: 1,
            chat: c.clone(),
            approver: None,
            approver_revision: None,
            approval_chat: None,
            delegation_revision: None,
        },
    )
    .await
    .unwrap();
    db.pooled = false;
    db.chat_send(
        &member,
        &w,
        &c,
        &message("Prepare my personal view", Some(&original.agent)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    observed(&db, &job, &read).await;
    p.id = uuid::Uuid::new_v4().to_string();
    db.save_personal_panel(&job, &p).await.unwrap();
    db.review_home_panel(
        &member,
        &w,
        &p.id,
        &Review {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
        },
    )
    .await
    .unwrap();
    db.chat_finish(&job, "Prepared", "complete").await.unwrap();
    db.pooled = true;
    assert!(
        super::agent_profiles::managed_access(&db.db, &member.id, &w, &original.agent, true)
            .await
            .is_err()
    );
    db.save_home_agreement(&member, &w, &p.id, &agreement(0, Some(60)))
        .await
        .unwrap();
    let tx = db.workspace_tx(&w).await.unwrap();
    super::automation_access::invalidate_inaccessible(&tx, &w)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let state = panel(&db, &member, &w).await;
    assert!(state["maintenance"]["eligible"] == true);
    let due = state["maintenance"]["next_ms"].as_i64().unwrap();
    assert_eq!(db.dispatch_schedules_at(due).await.unwrap(), 1);
    let accepted = db
        .update_home_panel(&member, &w, &p.id, &update(1))
        .await
        .unwrap();
    let updating = claim(&db, &w, accepted["reply"].as_str().unwrap()).await;
    assert_eq!(updating.requester, member.id);
    // Even a failed/no-read maintenance reply must be excluded from ordinary
    // recent context, including when there are no operation receipts to filter.
    db.chat_finish(&updating, "PRIVATE_MAINTENANCE_OUTPUT", "complete")
        .await
        .unwrap();
    let tx = db.read_tx(&w).await.unwrap();
    let recent = super::conversation_context::recent(
        &tx,
        super::conversation_context::RecentScope {
            workspace: &w,
            chat: &c,
            agent: &original.agent,
            thread: None,
            since_join: false,
            limit: 100,
            before: i64::MAX,
            request: "new-request",
        },
    )
    .await
    .unwrap();
    assert!(
        !json!(recent)
            .to_string()
            .contains("PRIVATE_MAINTENANCE_OUTPUT")
    );
    tx.commit().await.unwrap();
    assert_eq!(
        panel(&db, &member, &w).await["maintenance"]["status"],
        "failed"
    );
    assert_eq!(
        panel(&db, &member, &w).await["active"]["definition"]["sections"][0]["value"],
        "1"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn home_maintenance_age_is_source_completion_and_new_definition_fences_old_work() {
    let (db, owner, w, original, read, _, mut p) = setup().await;
    db.save_home_agreement(&owner, &w, &p.id, &agreement(0, None))
        .await
        .unwrap();
    let accepted = db
        .update_home_panel(&owner, &w, &p.id, &update(1))
        .await
        .unwrap();
    let job = claim(&db, &w, accepted["reply"].as_str().unwrap()).await;
    observed(&db, &job, &read).await;
    execute(
        &db.db,
        "UPDATE relay_control_actions SET updated=updated-4000 WHERE workspace=$1 AND job=$2",
        vec![w.clone().into(), job.reply.clone().into()],
    )
    .await
    .unwrap();
    let mut changed = refresh("2");
    changed.sections =
        serde_json::from_value(json!([{"kind":"metric","label":"Unreviewed meaning","value":"2"}]))
            .unwrap();
    assert!(db.refresh_home_panel(&job, &changed).await.is_err());
    db.refresh_home_panel(&job, &refresh("2")).await.unwrap();
    db.chat_finish(&job, "Saved", "complete").await.unwrap();
    let state = panel(&db, &owner, &w).await;
    assert_eq!(state["maintenance"]["status"], "delayed");
    assert!(
        state["active"]["saved_ms"].as_i64().unwrap()
            - state["active"]["checked_ms"].as_i64().unwrap()
            > 3900000
    );
    let accepted = db
        .update_home_panel(&owner, &w, &p.id, &update(1))
        .await
        .unwrap();
    let old = claim(&db, &w, accepted["reply"].as_str().unwrap()).await;
    observed(&db, &old, &read).await;
    db.chat_send(
        &owner,
        &w,
        &original.chat,
        &message("Change my panel purpose", Some(&original.agent)),
    )
    .await
    .unwrap();
    let reply=query(&db.db,"SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND state='queued' ORDER BY sequence DESC LIMIT 1",vec![w.clone().into()]).await.unwrap().unwrap().try_get::<String>("","reply").unwrap();
    let edit = claim(&db, &w, &reply).await;
    p.expected_revision = 1;
    p.purpose = "A newly reviewed purpose".into();
    db.save_personal_panel(&edit, &p).await.unwrap();
    assert!(db.refresh_home_panel(&old, &refresh("3")).await.is_err());
    assert!(
        db.update_home_panel(&owner, &w, &p.id, &update(1))
            .await
            .is_err()
    );
    assert_eq!(
        panel(&db, &owner, &w).await["maintenance"]["status"],
        "review_required"
    );
    db.chat_finish(&edit, "Changed definition", "complete")
        .await
        .unwrap();
    db.review_home_panel(
        &owner,
        &w,
        &p.id,
        &Review {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 2,
        },
    )
    .await
    .unwrap();
    assert!(panel(&db, &owner, &w).await["maintenance"]["eligible"] == true);
    let mut renewed = agreement(1, None);
    renewed.panel_revision = 2;
    db.save_home_agreement(&owner, &w, &p.id, &renewed)
        .await
        .unwrap();
    assert!(db.refresh_home_panel(&old, &refresh("3")).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn home_maintenance_v32_upgrade_preserves_panel_and_pending_question_without_starting_updates()
 {
    use sea_orm_migration::MigratorTrait;
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!(
        "relay_home_maintenance_upgrade_{}",
        uuid::Uuid::new_v4().simple()
    );
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
    migration::Migrator::up(&db.db, Some(32)).await.unwrap();
    let owner = user(&db, "maintenance-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    super::migration_fixture::legacy_request(&db, &owner, &w, &c, &a, "Existing saved panel work")
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let p:crate::panels::PanelDraft=serde_json::from_value(json!({"id":uuid::Uuid::new_v4().to_string(),"expected_revision":0,"title":"Retained panel","purpose":"Retained basis","template":"brief","sections":[{"kind":"text","heading":"Saved","body":"EXISTING_V32_BODY"}],"sources":[]})).unwrap();
    db.save_personal_panel(&job, &p).await.unwrap();
    // Seed the v32 human-reviewed pointer, whose historical API had no routine snapshots.
    execute(
        &db.db,
        "UPDATE relay_home_panels SET active_revision=1 WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), p.id.clone().into()],
    )
    .await
    .unwrap();
    let question = db
        .ask_question(&job, &super::attention_tests::ask())
        .await
        .unwrap();
    db.chat_finish(&job, "Waiting before upgrade", "complete")
        .await
        .unwrap();
    assert!(db.verify_schema().await.is_err());
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let state = panel(&db, &owner, &w).await;
    assert_eq!(
        state["active"]["definition"]["sections"][0]["body"],
        "EXISTING_V32_BODY"
    );
    assert_eq!(state["maintenance"]["revision"], 0);
    assert_eq!(state["maintenance"]["eligible"], false);
    let pending = query(
        &db.db,
        "SELECT state FROM relay_task_questions WHERE id=$1",
        vec![question["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(pending.try_get::<String>("", "state").unwrap(), "pending");
    assert!(
        query(&db.db, "SELECT panel FROM relay_home_maintenance", vec![])
            .await
            .unwrap()
            .is_none()
    );
    assert!(serde_json::from_value::<Agreement>(json!({"request":uuid::Uuid::new_v4().to_string(),"revision":0,"panel_revision":1,"enabled":true,"instructions":"Check","interval_seconds":60,"stale_seconds":60,"owner":"forged"})).is_err());
}
