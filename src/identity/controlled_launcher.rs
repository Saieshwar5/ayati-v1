//! Local supervisor using controller APIs; only this operator process holds bootstrap access.
use super::{
    assignment_launcher::LocalProvider,
    provider::random_token,
    worker_control::{client::Client, config, model::*},
};
use anyhow::{Context, Result};
use std::{path::Path, time::Duration};
use tokio::{process::Child, task::JoinSet};
use tokio_util::sync::CancellationToken;

pub(super) async fn serve(data: &Path) -> Result<()> {
    let url = config::url(
        &std::env::var("AYATI_WORKER_CONTROL_URL")
            .context("Local control launcher requires AYATI_WORKER_CONTROL_URL")?,
    )?;
    let key_file = std::path::PathBuf::from(
        std::env::var_os("AYATI_WORKER_CONTROL_KEY_FILE")
            .context("Local control launcher requires AYATI_WORKER_CONTROL_KEY_FILE")?,
    );
    let registration = Registration {
        pooled: false,
        executor: uuid::Uuid::new_v4().to_string(),
        incarnation: uuid::Uuid::new_v4().to_string(),
        token: random_token(),
    };
    register(&url, &key_file, &registration).await?;
    let client = Client::new(&url, &registration.token)?;
    let provider = LocalProvider::new(data)?;
    let capacity = super::worker_config::seconds("AYATI_ASSIGNMENT_CAPACITY", 2, 1, 16)? as usize;
    let stopping = CancellationToken::new();
    let signal = stopping.clone();
    let signal_task = tokio::spawn(async move {
        super::worker::shutdown_signal().await;
        signal.cancel();
    });
    let mut children = JoinSet::new();
    // Retain an uncertain reservation's exact key/token until reconciliation succeeds.
    let mut pending = None;
    let mut refresh_at = tokio::time::Instant::now() + Duration::from_secs(300);
    tracing::info!(executor=%registration.executor,"Controller API assignment launcher ready");
    while !stopping.is_cancelled() {
        if tokio::time::Instant::now() >= refresh_at {
            let refreshed = tokio::select! {
                result = register(&url,&key_file,&registration) => result,
                _ = stopping.cancelled() => break,
            };
            let delay = if refreshed.is_ok() { 300 } else { 5 };
            refresh_at = tokio::time::Instant::now() + Duration::from_secs(delay);
        }
        while children.len() < capacity && !stopping.is_cancelled() {
            let request = pending.get_or_insert_with(|| Reservation {
                request: uuid::Uuid::new_v4().to_string(),
                token: random_token(),
            });
            let reserved = tokio::select! {
                result = client.reserve(&registration,request) => result,
                _ = stopping.cancelled() => break,
            };
            let scope = match reserved {
                Ok(Some(scope)) => scope,
                Ok(None) => {
                    pending = None;
                    break;
                }
                Err(_) => {
                    refresh_at = tokio::time::Instant::now();
                    tracing::warn!(
                        "Controller assignment admission unavailable; reconciling same request"
                    );
                    break;
                }
            };
            let grant = Launch {
                url: url.clone(),
                scope: scope.clone(),
                token: request.token.clone(),
            };
            pending = None;
            match provider.launch(&scope.assignment, Some(&grant)).await {
                Ok(child) => {
                    let (client, registration, shutdown) =
                        (client.clone(), registration.clone(), stopping.clone());
                    children.spawn(async move {
                        let status = wait_child(child,&shutdown).await;
                        let reason = if shutdown.is_cancelled() { "interrupted" } else { "lost" };
                        if client.release(&registration,&scope,reason).await.is_err() {
                            tracing::warn!(assignment=%scope.assignment,"Child exit reconciliation deferred to durable lease");
                        }
                        tracing::info!(assignment=%scope.assignment,success=status.is_ok_and(|s|s.success()),"Controller-scoped child stopped");
                    });
                }
                Err(_) => {
                    let _ = client.release(&registration, &scope, "launch_failed").await;
                }
            }
        }
        tokio::select! {
            _ = stopping.cancelled() => {},
            _ = children.join_next(), if !children.is_empty() => {},
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
    }
    while children.join_next().await.is_some() {}
    signal_task.abort();
    Ok(())
}

async fn register(url: &str, key_file: &Path, registration: &Registration) -> Result<()> {
    let key = config::key(key_file)?;
    let _: serde_json::Value = Client::new(url, &key)?
        .retry("/v1/executors/register", registration, 5)
        .await?;
    Ok(())
}

async fn wait_child(
    mut child: Child,
    stopping: &CancellationToken,
) -> std::io::Result<std::process::ExitStatus> {
    tokio::select! {
        result = child.wait() => result,
        _ = stopping.cancelled() => {
            if let Some(pid)=child.id() { unsafe { libc::kill(pid as libc::pid_t,libc::SIGTERM); } }
            match tokio::time::timeout(Duration::from_secs(15),child.wait()).await {
                Ok(status) => status,
                Err(_) => { let _ = child.kill().await; child.wait().await }
            }
        }
    }
}
