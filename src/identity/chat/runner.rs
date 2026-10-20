use super::{store::Job, *};
use crate::{agent::Agent, files::Files, runtime::config::GooseConfig};

pub(super) async fn run(app: Shared, job: Job) {
    let result = execute(&app, &job).await;
    let (text, state) = match result {
        Ok(text) => (text, "complete"),
        Err(error) => {
            tracing::warn!(error=%error,"Shared conversation request did not complete");
            ("The agent could not complete this request. Check its setup or access and try again.".into(),"failed")
        }
    };
    if let Err(error) = app.store.chat_finish(&job, &text, state) {
        tracing::error!(%error,"Could not save shared reply");
    }
}
async fn execute(app: &Shared, job: &Job) -> Result<String> {
    let _slot = app.runtimes.chat_slots.acquire().await?;
    anyhow::ensure!(app.store.chat_job_allowed(job), "Request access changed");
    let state = app.runtimes.get(&job.workspace, super::port(app)).await?;
    let registry = state.workspace.context("Workspace unavailable")?;
    let profile = registry
        .profiles()?
        .into_iter()
        .find(|p| p.id == job.agent && p.state == "ready")
        .context("Agent unavailable")?;
    // Each shared request gets a fresh Goose context and no tools, private files,
    // memory or VM. Never point shared execution at the legacy owner's environment.
    let root = app.runtimes.chat_root(&job.workspace).join(&job.reply);
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
                !app.runtimes.stopping.is_cancelled() && app.store.chat_job_allowed(job),
                "Request access changed"
            );
            registry.agent(&job.agent)?;
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
