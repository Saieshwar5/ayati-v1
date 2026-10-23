//! Independently runnable, PostgreSQL-backed conversation worker. No HTTP listener.
use super::{
    chat::{execution, store::Job},
    database::Database,
    postgres::PgStore,
};
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub(super) fn embedded_mode(value: Option<&str>, postgres: bool) -> Result<bool> {
    Ok(super::worker_config::Mode::parse(value, postgres)? == super::worker_config::Mode::Embedded)
}

fn concurrency(value: Option<&str>) -> Result<usize> {
    let value: usize = value
        .unwrap_or("4")
        .parse()
        .context("AYATI_WORKER_CONCURRENCY must be an integer from 1 to 16")?;
    ensure!(
        (1..=16).contains(&value),
        "AYATI_WORKER_CONCURRENCY must be 1 to 16"
    );
    Ok(value)
}

pub async fn serve(data: &Path) -> Result<()> {
    ensure!(
        !matches!(
            super::worker_config::Mode::from_env(true)?,
            super::worker_config::Mode::Assigned | super::worker_config::Mode::Pooled
        ),
        "Assigned mode requires assignment-launcher or worker --assignment <id>"
    );
    let limit = concurrency(std::env::var("AYATI_WORKER_CONCURRENCY").ok().as_deref())?;
    let url = std::env::var("RELAY_DATABASE_URL")
        .context("The independent worker requires RELAY_DATABASE_URL; SQLite is not supported")?;
    let pg = PgStore::connect(&url).await?;
    pg.verify_schema().await?;
    let health = super::worker_health::Heartbeat::from_env()?;
    let root = data
        .join("chat-workers")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    let root = root.canonicalize()?;
    let stopping = CancellationToken::new();
    let signal = stopping.clone();
    let signal_task = tokio::spawn(async move {
        shutdown_signal().await;
        signal.cancel();
    });
    tracing::info!(concurrency = limit, "Independent shared-chat worker ready");
    dispatch(
        Arc::new(Database::Postgres(pg)),
        &root,
        &stopping,
        limit,
        health,
    )
    .await;
    signal_task.abort();
    execution::cleanup(&root).await;
    tracing::info!("Independent shared-chat worker stopped");
    Ok(())
}

async fn dispatch(
    db: Arc<Database>,
    root: &Path,
    stopping: &CancellationToken,
    limit: usize,
    mut health: Option<super::worker_health::Heartbeat>,
) {
    let Database::Postgres(pg) = db.as_ref() else {
        unreachable!()
    };
    let mut tasks = JoinSet::new();
    while !stopping.is_cancelled() {
        if let Some(health) = &mut health {
            health.tick(pg).await;
        }
        while tasks.len() < limit && !stopping.is_cancelled() {
            let claimed = tokio::select! {
                _ = stopping.cancelled() => break,
                result = pg.claim_job() => result,
            };
            match claimed {
                Ok(Some(job)) => {
                    let db = db.clone();
                    let root = root.to_path_buf();
                    let stopping = stopping.clone();
                    tasks.spawn(async move { run(db, root, stopping, job).await });
                }
                Ok(None) => break,
                Err(_) => {
                    tracing::warn!("Shared work database unavailable; dispatch will retry");
                    break;
                }
            }
        }
        tokio::select! {
            _ = stopping.cancelled() => {},
            result = tasks.join_next(), if !tasks.is_empty() => {
                if result.is_some_and(|result| result.is_err()) {
                    tracing::error!("Shared job panicked; its lease will expire for recovery");
                }
            },
            _ = tokio::time::sleep(Duration::from_millis(500)) => {},
        }
    }
    // Stop Goose before releasing each claim; do not abort source-bearing tasks.
    while let Some(result) = tasks.join_next().await {
        if result.is_err() {
            tracing::error!(
                "Shared job panicked during shutdown; lease recovery remains available"
            );
        }
    }
}

async fn run(db: Arc<Database>, root: PathBuf, stopping: CancellationToken, job: Job) {
    tracing::info!(workspace=%job.workspace, agent=%job.agent, reply=%job.reply, "Claimed shared-chat job");
    // Unique per-process/per-attempt roots need no API files or local registry.
    let attempt = root.join(uuid::Uuid::new_v4().to_string());
    let result = async {
        ensure!(db.chat_job_allowed(&job).await, "Request access changed");
        let profile = execution::managed_profile(&db, &job)
            .await?
            .context("Independent workers require a PostgreSQL-managed agent profile")?;
        execution::execute(&db, &job, &attempt, &stopping, profile).await
    }
    .await;
    execution::cleanup(&attempt).await;
    let Database::Postgres(pg) = db.as_ref() else {
        unreachable!()
    };
    if stopping.is_cancelled() {
        if pg.release_job(&job).await.is_err() {
            tracing::warn!(reply=%job.reply, "Could not release stopped job; lease expiry will recover it");
        }
        return;
    }
    let (text, state) = match result {
        Ok(text) => (text, "complete"),
        Err(_) => {
            tracing::warn!(reply=%job.reply, "Shared-chat execution failed");
            ("The agent could not complete this request. Check its setup or access and try again.".into(), "failed")
        }
    };
    if pg.chat_finish(&job, &text, state).await.is_err() {
        tracing::error!(reply=%job.reply, "Could not save shared reply; lease recovery remains available");
    }
}

pub(super) async fn shutdown_signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = terminate.recv() => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn execution_mode_is_explicit_and_external_requires_postgres() {
        assert!(embedded_mode(None, false).unwrap());
        assert!(embedded_mode(Some("embedded"), true).unwrap());
        assert!(!embedded_mode(Some("external"), true).unwrap());
        assert!(embedded_mode(Some("external"), false).is_err());
        assert!(embedded_mode(Some("typo"), true).is_err());
    }
    #[test]
    fn worker_capacity_is_bounded() {
        assert_eq!(concurrency(None).unwrap(), 4);
        assert_eq!(concurrency(Some("1")).unwrap(), 1);
        for value in ["0", "17", "many", "-1"] {
            assert!(concurrency(Some(value)).is_err());
        }
    }
}
