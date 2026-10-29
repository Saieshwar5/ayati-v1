use super::{tests::*, *};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    rehearsals::model::Start,
};
use serde_json::json;
pub(super) async fn setup() -> (PgStore, User, String, String, String) {
    let db = fixture().await;
    let owner = user(&db, "sample-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let input = SaveAgent {
        id: agent.clone(),
        name: "Pool agent".into(),
        role: "Bounded fixture".into(),
        model: "fixture".into(),
        scope: "workspace".into(),
        revision: 1,
        settings: Settings {
            provider: "openai".into(),
            memory: true,
            successful_outcome: Some("A clear sourced draft".into()),
            routine: Some("Check the supplied facts".into()),
            ..Default::default()
        },
    };
    db.save_agent_profile(&owner, &w, &input).await.unwrap();
    (db, owner, w, chat, agent)
}
pub(super) fn start() -> Start {
    Start {
        id: uuid::Uuid::new_v4().to_string(),
        agent_revision: 2,
        task: "Draft a useful delivery response".into(),
        sources: vec![],
    }
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn rehearsal_exact_start_and_fenced_completion_never_create_business_work_or_expose_prepared_context()
 {
    let (db, owner, w, _, agent) = setup().await;
    let input = start();
    let before = db.company_counts(&owner, &w).await.unwrap();
    let (a, b) = tokio::join!(
        db.start_rehearsal(&owner, &w, &agent, &input),
        db.start_rehearsal(&owner, &w, &agent, &input)
    );
    let first = a.unwrap();
    assert_eq!(first, b.unwrap());
    assert_eq!(first["state"], "queued");
    assert!(first.get("prepared").is_none());
    assert!(first.get("claim").is_none());
    assert_eq!(before, db.company_counts(&owner, &w).await.unwrap());
    let mut changed = input.clone();
    changed.task = "Different sample".into();
    assert!(
        db.start_rehearsal(&owner, &w, &agent, &changed)
            .await
            .is_err()
    );
    assert!(
        db.start_rehearsal(&owner, &w, &agent, &start())
            .await
            .is_err()
    );
    let claim = db.claim_rehearsal().await.unwrap().unwrap();
    assert_eq!(claim.id, input.id);
    assert!(!claim.prepared.control_tools);
    assert!(
        claim
            .prepared
            .instructions
            .contains("A clear sourced draft")
    );
    let root = tempfile::tempdir().unwrap();
    let config =
        crate::identity::chat::execution::runtime_config(&claim.prepared, root.path(), None)
            .unwrap();
    assert_eq!(
        config.session_meta().unwrap()["enabledExtensions"],
        json!([])
    );
    assert!(!config.proof_tool && config.control_tools.is_none());
    let mut stale = claim.clone();
    stale.token = uuid::Uuid::new_v4().to_string();
    assert!(!db.heartbeat_rehearsal(&stale).await.unwrap());
    assert!(
        !db.finish_rehearsal(&stale, Some("Must not publish"), false)
            .await
            .unwrap()
    );
    assert!(db.heartbeat_rehearsal(&claim).await.unwrap());
    assert!(
        db.finish_rehearsal(&claim, Some("A saved draft"), false)
            .await
            .unwrap()
    );
    assert!(
        !db.finish_rehearsal(&claim, Some("Late replacement"), false)
            .await
            .unwrap()
    );
    let complete = db.rehearsal(&owner, &w, &agent, &input.id).await.unwrap();
    assert_eq!(complete["output"], "A saved draft");
    assert_eq!(complete["state"], "completed");
    assert!(query(&db.db,"SELECT id FROM relay_agent_rehearsals WHERE workspace=$1 AND id=$2 AND prepared IS NULL AND claim IS NULL",vec![w.clone().into(),input.id.clone().into()]).await.unwrap().is_some());
    assert_eq!(
        db.start_rehearsal(&owner, &w, &agent, &input)
            .await
            .unwrap(),
        complete
    );
    assert!(db.claim_rehearsal().await.unwrap().is_none());
    assert!(db.claim_job().await.unwrap().is_none());
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn rehearsal_stop_and_expiry_are_durable_and_cannot_replay_inference_or_publish_late_results()
{
    let (db, owner, w, _, agent) = setup().await;
    let queued = start();
    db.start_rehearsal(&owner, &w, &agent, &queued)
        .await
        .unwrap();
    assert_eq!(
        db.stop_rehearsal(&owner, &w, &agent, &queued.id)
            .await
            .unwrap()["state"],
        "cancelled"
    );
    assert_eq!(
        db.stop_rehearsal(&owner, &w, &agent, &queued.id)
            .await
            .unwrap()["state"],
        "cancelled"
    );
    assert!(db.claim_rehearsal().await.unwrap().is_none());
    let active = start();
    db.start_rehearsal(&owner, &w, &agent, &active)
        .await
        .unwrap();
    let claim = db.claim_rehearsal().await.unwrap().unwrap();
    db.stop_rehearsal(&owner, &w, &agent, &active.id)
        .await
        .unwrap();
    assert!(!db.heartbeat_rehearsal(&claim).await.unwrap());
    assert!(
        !db.finish_rehearsal(&claim, Some("Must not publish after Stop"), false)
            .await
            .unwrap()
    );
    let expired = start();
    db.start_rehearsal(&owner, &w, &agent, &expired)
        .await
        .unwrap();
    let claim = db.claim_rehearsal().await.unwrap().unwrap();
    execute(
        &db.db,
        "UPDATE relay_agent_rehearsals SET lease_until=0 WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), expired.id.clone().into()],
    )
    .await
    .unwrap();
    assert!(!db.heartbeat_rehearsal(&claim).await.unwrap());
    db.recover_rehearsals().await.unwrap();
    assert_eq!(
        db.rehearsal(&owner, &w, &agent, &expired.id).await.unwrap()["state"],
        "interrupted"
    );
    assert!(db.claim_rehearsal().await.unwrap().is_none());
    assert!(
        !db.finish_rehearsal(&claim, Some("Must not publish after expiry"), false)
            .await
            .unwrap()
    );
    let replay = db
        .start_rehearsal(&owner, &w, &agent, &expired)
        .await
        .unwrap();
    assert_eq!(replay["state"], "interrupted");
    assert_eq!(
        db.managed_agent(&owner, &w, &agent, true).await.unwrap()["settings"]["paused"],
        false
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn rehearsal_capacity_is_atomic_across_controller_claimers_and_workspaces() {
    let (db, owner, w, _, agent) = setup().await;
    db.start_rehearsal(&owner, &w, &agent, &start())
        .await
        .unwrap();
    for _ in 0..2 {
        let another = workspace(&db, &owner).await;
        let (_, a) = super::executor_slot_tests::agent(&db, &owner, &another).await;
        let mut input = start();
        input.agent_revision = 1;
        db.start_rehearsal(&owner, &another, &a, &input)
            .await
            .unwrap();
    }
    let (a, b, c) = tokio::join!(
        db.claim_rehearsal(),
        db.claim_rehearsal(),
        db.claim_rehearsal()
    );
    let claimed = [a.unwrap(), b.unwrap(), c.unwrap()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(claimed.len(), 2);
    assert_ne!(claimed[0].id, claimed[1].id);
    assert!(
        db.finish_rehearsal(&claimed[0], Some("Completed sample"), false)
            .await
            .unwrap()
    );
    let next = db.claim_rehearsal().await.unwrap().unwrap();
    assert!(!claimed.iter().any(|c| c.id == next.id));
    assert!(db.claim_rehearsal().await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn job_guidance_survives_older_clients_and_enters_normal_prompt_without_granting_tools() {
    let (db, owner, w, chat, agent) = setup().await;
    let mut input = SaveAgent {
        id: agent.clone(),
        name: "Updated name".into(),
        role: "Help with deliveries".into(),
        model: "fixture".into(),
        scope: "workspace".into(),
        revision: 2,
        settings: Settings {
            provider: "openai".into(),
            ..Default::default()
        },
    };
    let updated = db.save_agent_profile(&owner, &w, &input).await.unwrap();
    assert_eq!(
        updated["settings"]["successful_outcome"],
        "A clear sourced draft"
    );
    assert_eq!(updated["settings"]["routine"], "Check the supplied facts");
    assert_eq!(
        db.save_agent_profile(&owner, &w, &input).await.unwrap(),
        updated
    );
    db.chat_send(
        &owner,
        &w,
        &chat,
        &message("Normal work with saved guidance", Some(&agent)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let prepared = crate::identity::chat::execution::prepare(
        &crate::identity::database::Database::Postgres(db.clone()),
        &job,
        crate::identity::chat::execution::Profile {
            name: input.name.clone(),
            role: input.role.clone(),
            model: input.model.clone(),
        },
        45,
    )
    .await
    .unwrap();
    assert!(prepared.instructions.contains("A clear sourced draft"));
    assert!(prepared.instructions.contains("Check the supplied facts"));
    assert!(!db.control_enabled(&w, &agent, &chat).await.unwrap());
    db.chat_finish(&job, "Normal draft saved", "complete")
        .await
        .unwrap();
    input.revision = 3;
    input.settings.successful_outcome = Some(String::new());
    let cleared = db.save_agent_profile(&owner, &w, &input).await.unwrap();
    assert_eq!(cleared["settings"]["successful_outcome"], "");
    assert_eq!(cleared["settings"]["routine"], "Check the supplied facts");
}
