//! One durable assignment, one supervised Goose attempt, then process exit.
use super::{chat::execution, database::Database, postgres::PgStore, worker_config::Mode};
use anyhow::{Context, Result, ensure};
use std::{path::Path, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub async fn serve(data: &Path, id: &str) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id),
        "Assignment ID must be a canonical UUID"
    );
    ensure!(
        Mode::from_env(true)? == Mode::Assigned,
        "Assignment workers require AYATI_CHAT_EXECUTION=assigned"
    );
    let pg = PgStore::connect(
        &std::env::var("RELAY_DATABASE_URL")
            .context("Assignment workers require RELAY_DATABASE_URL")?,
    )
    .await?;
    pg.verify_schema().await?;
    let resource = format!("local:{}", std::process::id());
    let assignment = pg
        .start_assignment(id, &resource)
        .await?
        .context("Assignment unavailable, expired or already started")?;
    let db = Arc::new(Database::Postgres(pg));
    let root = data
        .join("chat-workers")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    let mut health = super::worker_health::Heartbeat::from_env()?;
    let stopping = CancellationToken::new();
    let execution_cancel = stopping.child_token();
    let signal = stopping.clone();
    let signal_task = tokio::spawn(async move {
        super::worker::shutdown_signal().await;
        signal.cancel();
    });
    let heartbeat_db = db.clone();
    let heartbeat_assignment = assignment.clone();
    let heartbeat_cancel = execution_cancel.clone();
    let watchdog = tokio::spawn(async move {
        let Database::Postgres(pg) = heartbeat_db.as_ref() else {
            unreachable!()
        };
        let period = Duration::from_secs(heartbeat_assignment.lease_seconds / 3);
        let mut next = tokio::time::Instant::now();
        loop {
            if let Some(health) = &mut health {
                health.tick(pg).await;
            }
            if tokio::time::Instant::now() >= next {
                if !matches!(
                    tokio::time::timeout(period, pg.renew_assignment(&heartbeat_assignment)).await,
                    Ok(Ok(true))
                ) {
                    heartbeat_cancel.cancel();
                    break;
                }
                next = tokio::time::Instant::now() + period;
            }
            tokio::select! {
                _ = heartbeat_cancel.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    });
    tracing::info!(assignment=%assignment.id, reply=%assignment.job.reply, "Assignment started");
    let result = async {
        let profile = execution::managed_profile(&db, &assignment.job)
            .await?
            .context("Assignment requires a PostgreSQL-managed agent profile")?;
        execution::execute_bounded(
            &db,
            &assignment.job,
            &root,
            &execution_cancel,
            profile,
            assignment.run_seconds,
        )
        .await
    }
    .await;
    // execute_bounded has stopped/reaped Goose before a claim may be released.
    let Database::Postgres(pg) = db.as_ref() else {
        unreachable!()
    };
    let outcome = if stopping.is_cancelled() {
        if pg
            .release_assignment(id, "interrupted", Some(&resource))
            .await
            .is_err()
        {
            tracing::warn!(assignment=%id, "Could not release stopped assignment; lease expiry will recover it");
        }
        Err(anyhow::anyhow!("Assignment interrupted"))
    } else {
        let (text, state) = match &result {
            Ok(text) => (text.as_str(), "complete"),
            Err(_) => (
                "The agent could not complete this request. Check its setup or access and try again.",
                "failed",
            ),
        };
        match pg.finish_claimed_job(&assignment.job, text, state).await {
            Ok(true) if result.is_ok() => Ok(()),
            Ok(true) => Err(anyhow::anyhow!(
                "Assignment execution failed; outcome saved"
            )),
            Ok(false) => Err(anyhow::anyhow!("Assignment no longer owns this request")),
            Err(_) => Err(anyhow::anyhow!(
                "Could not save assignment outcome; recovery remains available"
            )),
        }
    };
    watchdog.abort();
    let _ = watchdog.await;
    signal_task.abort();
    execution::cleanup(&root).await;
    tracing::info!(assignment=%id, success=outcome.is_ok(), "Assignment worker exiting");
    outcome
}
