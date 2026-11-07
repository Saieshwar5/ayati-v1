//! MG1: with a model catalog, agents choose Cuckoo's models and never a provider;
//! gateway executors that serve any catalog model take any agent's work.
use super::executor_slot_tests::{report, reserve, setup};
use super::settings::workspace::{ModelChoice, WorkspaceSettingsInput};
use super::tests::*;
use crate::identity::agents::config::{SaveAgent, Settings};
use crate::identity::executor::model::Profile;

fn catalog(dir: &std::path::Path) -> crate::identity::models::catalog::Files {
    let providers = dir.join("providers.json");
    std::fs::write(&providers, r#"[{"provider":"fireworks","base_url":"https://api.fireworks.ai/inference/v1","key_file":"/k"}]"#).unwrap();
    let file = dir.join("catalog.json");
    std::fs::write(&file, r#"{"models":[
      {"id":"llama-3.3-70b","name":"Llama 3.3 70B","routes":[{"provider":"fireworks","model":"accounts/fireworks/models/llama-v3p3-70b-instruct"}]},
      {"id":"old","name":"Old","status":"retiring","replacement":"llama-3.3-70b","routes":[{"provider":"fireworks","model":"old"}]}]}"#).unwrap();
    crate::identity::models::catalog::Files {
        catalog: file,
        providers,
    }
}
fn new_agent(model: &str, provider: &str) -> SaveAgent {
    SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Catalog agent".into(),
        role: "Answers".into(),
        model: model.into(),
        audience: "selected".into(),
        people: None,
        managers: None,
        revision: 0,
        settings: Settings {
            provider: provider.into(),
            ..Default::default()
        },
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn with_a_catalog_agents_choose_cuckoo_models_not_providers() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = fixture().await;
    db.model_catalog = Some(catalog(dir.path()));
    let owner = user(&db, "catalog-owner").await;
    let w = workspace(&db, &owner).await;
    let said = |e: anyhow::Error| e.to_string();
    assert!(
        said(
            db.save_agent_profile(&owner, &w, &new_agent("gpt-9", ""))
                .await
                .unwrap_err()
        )
        .contains("isn't one of the models")
    );
    assert!(
        said(
            db.save_agent_profile(&owner, &w, &new_agent("llama-3.3-70b", "openai"))
                .await
                .unwrap_err()
        )
        .contains("Cuckoo picks who serves it")
    );
    assert!(
        said(
            db.save_agent_profile(&owner, &w, &new_agent("old", ""))
                .await
                .unwrap_err()
        )
        .contains("choose Llama 3.3 70B")
    );
    db.save_agent_profile(&owner, &w, &new_agent("llama-3.3-70b", ""))
        .await
        .unwrap();
    // Admins limit the workspace to catalog models only.
    let settings = |models: Vec<ModelChoice>| WorkspaceSettingsInput {
        timezone: "Asia/Kolkata".into(),
        colour: None,
        approval_backup_hours: 24,
        default_model: models.first().map(|m| m.model.clone()),
        models,
    };
    let catalog_choice = ModelChoice {
        provider: String::new(),
        model: "llama-3.3-70b".into(),
    };
    assert!(
        db.save_workspace_settings(
            &owner,
            &w,
            &settings(vec![ModelChoice {
                provider: "openai".into(),
                model: "gpt-4o".into()
            }])
        )
        .await
        .is_err()
    );
    assert!(
        db.save_workspace_settings(
            &owner,
            &w,
            &settings(vec![ModelChoice {
                provider: String::new(),
                model: "gpt-9".into()
            }])
        )
        .await
        .is_err()
    );
    db.save_workspace_settings(&owner, &w, &settings(vec![catalog_choice]))
        .await
        .unwrap();
    let (models, default) = db.workspace_models(&owner, &w).await.unwrap();
    assert_eq!(models[0]["model"], "llama-3.3-70b");
    assert_eq!(default.as_deref(), Some("llama-3.3-70b"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_gateway_executor_serving_any_model_takes_any_agents_work() {
    let (db, owner, w, _c, _a, e, mut slot) = setup().await;
    // An agent whose model no executor names; only a gateway slot can serve it.
    let catalog_agent = new_agent("llama-3.3-70b", "");
    db.save_agent_profile(&owner, &w, &catalog_agent)
        .await
        .unwrap();
    let conversation = chat(&db, &owner, &w, vec![], vec![catalog_agent.id.clone()]).await;
    db.chat_send(
        &owner,
        &w,
        &conversation,
        &message("Hello", Some(&catalog_agent.id)),
    )
    .await
    .unwrap();
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    assert!(
        reserve(&db, &e).await.is_none(),
        "a fixture slot doesn't serve llama"
    );
    slot.profile = Profile::any();
    slot.generation = 2;
    slot.token = crate::identity::provider::random_token();
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    let scope = reserve(&db, &e).await.expect("the gateway slot takes it");
    assert_eq!(scope.agent, catalog_agent.id);
    // An invalid "any" profile is refused.
    let mut bad = slot.clone();
    bad.profile = Profile {
        provider: "gateway".into(),
        model: "llama".into(),
    };
    bad.slot = uuid::Uuid::new_v4().to_string();
    assert!(report(&db, &e, vec![slot.clone(), bad]).await.is_err());
}
