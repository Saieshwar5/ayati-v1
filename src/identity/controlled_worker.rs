//! Execute one controller-scoped context without a PostgreSQL or WorkOS credential.
use super::{
    chat::execution,
    worker_control::{client::Client, model::*},
};
use anyhow::{Context, Result, ensure};
use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

pub async fn serve(data: &Path) -> Result<()> {
    for name in [
        "RELAY_DATABASE_URL",
        "RELAY_SIMULATION_DATABASE_URL",
        "WORKOS_API_KEY",
        "WORK_OS_API_KEY",
        "AYATI_WORKER_CONTROL_KEY_FILE",
    ] {
        ensure!(
            std::env::var_os(name).is_none(),
            "Controller-assigned workers must not receive {name}"
        );
    }
    let mut input = Vec::new();
    tokio::io::stdin()
        .take(16385)
        .read_to_end(&mut input)
        .await?;
    ensure!(
        input.len() <= 16384,
        "Assignment envelope exceeds protocol limit"
    );
    let launch: Launch = serde_json::from_slice(&input)
        .context("Expected a private assignment envelope on stdin")?;
    for id in [
        &launch.scope.executor,
        &launch.scope.incarnation,
        &launch.scope.slot,
        &launch.scope.assignment,
    ] {
        super::worker_control::model::uuid(id)?;
    }
    let client = Client::new(&launch.url, &launch.token)?;
    let request = AssignedRequest {
        scope: launch.scope,
        session: uuid::Uuid::new_v4().to_string(),
    };
    let stopping = CancellationToken::new();
    let signal = stopping.clone();
    let signal_task = tokio::spawn(async move {
        super::worker::shutdown_signal().await;
        signal.cancel();
    });
    let root = data
        .join("chat-workers")
        .join(uuid::Uuid::new_v4().to_string());
    let outcome = super::worker_control::attempt::run(
        client,
        request,
        stopping,
        |context, cancel| async move {
            let result = execution::execute_prepared(&context, &root, &cancel).await;
            execution::cleanup(&root).await;
            result
        },
    )
    .await;
    signal_task.abort();
    outcome
}
