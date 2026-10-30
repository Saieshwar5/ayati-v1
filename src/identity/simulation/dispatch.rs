//! Scripted replies for the company simulation, claimed inside the controller.
//! Product controllers never run agents here; they dispatch to pooled executors.
use super::*;
use crate::identity::chat::model::Job;

pub(in crate::identity) fn start(app: Shared) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = app.runtimes.stopping.cancelled() => break,
                _ = app.runtimes.chat_wake.notified() => {},
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
            }
            if app
                .simulation
                .as_ref()
                .is_some_and(|sim| sim.paused.load(std::sync::atomic::Ordering::Acquire))
            {
                continue;
            }
            // Reserve capacity before claiming: never lease jobs waiting for a local slot.
            while let Ok(slot) = app.runtimes.chat_slots.clone().try_acquire_owned() {
                match app.store.claim_job().await {
                    Ok(Some(job)) => {
                        tokio::spawn(run(app.clone(), job, slot));
                    }
                    Ok(None) => break,
                    Err(_) => {
                        tracing::warn!("Shared work database unavailable; dispatch will retry");
                        break;
                    }
                }
            }
        }
    });
}

async fn run(app: Shared, job: Job, slot: tokio::sync::OwnedSemaphorePermit) {
    let (text, state) = match reply(&app, &job).await {
        Ok(text) => (text, "complete"),
        Err(_) => (
            "The agent could not complete this request. Check its setup or access and try again."
                .into(),
            "failed",
        ),
    };
    if app.runtimes.stopping.is_cancelled() {
        let _ = app.store.release_job(&job).await;
    } else if app.store.chat_finish(&job, &text, state).await.is_err() {
        tracing::error!("Could not save shared reply");
    }
    drop(slot);
    app.runtimes.chat_wake.notify_one();
}

async fn reply(app: &Shared, job: &Job) -> Result<String> {
    anyhow::ensure!(
        app.store.chat_job_allowed(job).await,
        "Request access changed"
    );
    let sources = app.store.library_job_context(job).await?;
    let text = execute(app, job).await?;
    Ok(format!(
        "{text}{}",
        library::context::citations(&job.workspace, &sources)
    ))
}
