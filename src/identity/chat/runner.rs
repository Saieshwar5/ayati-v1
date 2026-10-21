use super::{store::Job, *};
use crate::{agent::Agent, files::Files, runtime::config::GooseConfig};

pub(super) async fn run(app: Shared, job: Job) {
    let Ok(slot) = app.runtimes.chat_slots.clone().acquire_owned().await else {
        return;
    };
    run_claimed(app, job, slot).await;
}
pub(super) async fn run_claimed(app: Shared, job: Job, slot: tokio::sync::OwnedSemaphorePermit) {
    let result = execute(&app, &job).await;
    let (text, state) = match result {
        Ok(text) => (text, "complete"),
        Err(error) => {
            tracing::warn!(error=%error,"Shared conversation request did not complete");
            ("The agent could not complete this request. Check its setup or access and try again.".into(),"failed")
        }
    };
    if let Err(error) = app.store.chat_finish(&job, &text, state).await {
        tracing::error!(%error,"Could not save shared reply");
    }
    drop(slot);
    app.runtimes.chat_wake.notify_one();
}
async fn execute(app: &Shared, job: &Job) -> Result<String> {
    anyhow::ensure!(
        app.store.chat_job_allowed(job).await,
        "Request access changed"
    );
    if app.simulation.is_some() {
        return super::super::simulation::execute(app, job).await;
    }
    let shared = if let super::super::database::Database::Postgres(pg) = &app.store {
        pg.shared_agents(&job.workspace)
            .await?
            .into_iter()
            .find(|p| p["id"] == job.agent)
    } else {
        None
    };
    let state = app.runtimes.get(&job.workspace, super::port(app)).await?;
    let registry = state.workspace.context("Workspace unavailable")?;
    let profile = if let Some(p) = shared {
        crate::workspace::registry::AgentProfile {
            id: job.agent.clone(),
            name: p["name"].as_str().unwrap_or("Agent").into(),
            role: p["role"].as_str().unwrap_or("").into(),
            model: p["model"].as_str().unwrap_or("").into(),
            memory: false,
            browser: false,
            state: "ready".into(),
            detail: String::new(),
            attempt: String::new(),
        }
    } else {
        registry
            .profiles()?
            .into_iter()
            .find(|p| p.id == job.agent && p.state == "ready")
            .context("Agent unavailable")?
    };
    // Each shared request gets a fresh Goose context and no tools, private files,
    // memory or VM. Never point shared execution at the legacy owner's environment.
    let mut root = app.runtimes.chat_root(&job.workspace).join(&job.reply);
    if let Some(claim) = &job.claim {
        // A recovered attempt needs fresh runtime state; stale workers must not share files.
        root = root.join(claim);
    }
    std::fs::create_dir_all(&root)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    let mut config = GooseConfig::from_env(&root, &profile.model);
    config.agent_id = job.agent.clone();
    config.role = profile.role;
    config.vm_manifest = None;
    config.browser = false;
    config.memory = false;
    config.platform = None;
    config.proof_tool = false;
    anyhow::ensure!(
        config.configured(),
        "Shared chat needs the configured Goose binary and provider"
    );
    let store = Arc::new(crate::storage::Store::open(&root.join("chat.sqlite"))?);
    let files = Arc::new(Files::new(&root, store.clone())?);
    let agent = Agent::start_goose(store.clone(), files, config, None);
    let result = async {
        let task = agent.submit_once(&job.prompt, Some(&job.reply))?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
        loop {
            anyhow::ensure!(
                !app.runtimes.stopping.is_cancelled() && app.store.chat_job_allowed(job).await,
                "Request access changed"
            );
            // Shared profiles do not require a private VM-backed registry entry.
            let task = store.task(&task.id)?.context("Task missing")?;
            if !["queued", "running"].contains(&task.status.as_str()) {
                anyhow::ensure!(
                    task.status == "complete" || task.status == "completed",
                    "Request failed"
                );
                return store
                    .messages()?
                    .into_iter()
                    .rev()
                    .find(|m| {
                        m.task_id == task.id && m.role == "assistant" && m.state == "complete"
                    })
                    .map(|m| m.content)
                    .context("No completed reply");
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "Shared reply timed out"
            );
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }
    .await;
    agent.begin_shutdown();
    agent.stop_all()?;
    agent.wait_until_idle().await;
    agent.shutdown_runtime().await?;
    result
}
