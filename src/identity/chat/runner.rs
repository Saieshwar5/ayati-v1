use super::{execution, store::Job, *};

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
        Err(_) => (
            "The agent could not complete this request. Check its setup or access and try again."
                .into(),
            "failed",
        ),
    };
    if app.runtimes.stopping.is_cancelled() {
        if let super::super::database::Database::Postgres(pg) = &app.store {
            let _ = pg.release_job(&job).await;
        }
    } else if app.store.chat_finish(&job, &text, state).await.is_err() {
        tracing::error!("Could not save shared reply");
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
        let sources = super::super::collaboration::pg(app)?
            .library_job_context(job)
            .await?;
        let text = super::super::simulation::execute(app, job).await?;
        return Ok(format!(
            "{text}{}",
            super::super::library::context::citations(&job.workspace, &sources)
        ));
    }
    let profile = match execution::managed_profile(&app.store, job).await? {
        Some(profile) => profile,
        None => {
            // Only the embedded compatibility path can consult the local registry.
            let state = app.runtimes.get(&job.workspace, super::port(app)).await?;
            let profile = state
                .workspace
                .context("Workspace unavailable")?
                .profiles()?
                .into_iter()
                .find(|p| p.id == job.agent && p.state == "ready")
                .context("Agent unavailable")?;
            execution::Profile {
                name: profile.name,
                role: profile.role,
                model: profile.model,
            }
        }
    };
    let mut root = app.runtimes.chat_root(&job.workspace).join(&job.reply);
    if let Some(claim) = &job.claim {
        root = root.join(claim);
    }
    let result = execution::execute(&app.store, job, &root, &app.runtimes.stopping, profile).await;
    execution::cleanup(&root).await;
    result
}
