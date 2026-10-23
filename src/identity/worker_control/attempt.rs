//! Shared start/lease/completion lifecycle for cold workers and prepared executor slots.
use super::{
    client::{Client, Unavailable},
    model::*,
};
use crate::identity::chat::execution;
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
#[derive(Deserialize)]
struct Started {
    context: execution::Prepared,
    lease_seconds: u64,
}
#[derive(Deserialize)]
struct Lease {
    lease_seconds: u64,
}

pub(crate) async fn run<F, Fut>(
    client: Client,
    request: AssignedRequest,
    stopping: CancellationToken,
    execute: F,
) -> Result<()>
where
    F: FnOnce(execution::Prepared, CancellationToken) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    let base = format!("/v1/assignments/{}", request.scope.assignment);
    let began = tokio::time::Instant::now();
    let started: Started = client.retry(&format!("{base}/start"), &request, 3).await?;
    ensure!(
        started.context.agent == request.scope.agent
            && started.context.reply == request.scope.reply
            && started.context.workspace == request.scope.workspace,
        "Controller context scope mismatch"
    );
    ensure!(
        (6..=300).contains(&started.lease_seconds),
        "Invalid assignment lease"
    );
    let initial_expiry = began + Duration::from_secs(started.lease_seconds - 1);
    ensure!(
        tokio::time::Instant::now() < initial_expiry,
        "Assignment acknowledgment arrived after its safe lease window"
    );
    let execution_cancel = stopping.child_token();
    let (watch_client, watch_request, watch_cancel) =
        (client.clone(), request.clone(), execution_cancel.clone());
    let renew_path = format!("{base}/renew");
    let watchdog = tokio::spawn(async move {
        let mut valid_until = initial_expiry;
        loop {
            let sent = tokio::time::Instant::now();
            let renewal = tokio::time::timeout_at(
                valid_until,
                watch_client.post::<Lease>(&renew_path, &watch_request),
            )
            .await;
            match renewal {
                Ok(Ok(lease)) if (6..=300).contains(&lease.lease_seconds) => {
                    valid_until = sent + Duration::from_secs(lease.lease_seconds - 1);
                }
                Ok(Err(error))
                    if error.is::<Unavailable>() && tokio::time::Instant::now() < valid_until => {}
                _ => {
                    tracing::warn!(assignment=%watch_request.scope.assignment,"Assignment renewal denied or safe lease expired; stopping harness");
                    watch_cancel.cancel();
                    break;
                }
            }
            if tokio::time::Instant::now() >= valid_until {
                watch_cancel.cancel();
                break;
            }
            tokio::select! {
                _ = watch_cancel.cancelled() => break,
                _ = tokio::time::sleep_until(valid_until) => { watch_cancel.cancel(); break; },
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        }
    });
    tracing::info!(assignment=%request.scope.assignment,"Controller-scoped assignment started");
    let result = execute(started.context, execution_cancel.clone())
        .await
        .and_then(|text| {
            ensure!(
                text.len() <= 65536 && !text.contains('\0'),
                "Reply exceeds the controller protocol limit"
            );
            Ok(text)
        });
    // The harness has stopped/reaped before terminal publication or launcher release.
    let outcome = if stopping.is_cancelled() {
        Err(anyhow::anyhow!(
            "Assignment interrupted; launcher will reconcile ownership"
        ))
    } else {
        let completion = Completion {assignment:request.clone(),state:if result.is_ok(){"complete"}else{"failed"}.into(),
            text:result.as_deref().unwrap_or("The agent could not complete this request. Check its setup or access and try again.").into()};
        match client
            .retry::<serde_json::Value>(&format!("{base}/complete"), &completion, 5)
            .await
        {
            Ok(receipt) if receipt["state"] == "complete" && result.is_ok() => Ok(()),
            Ok(_) => Err(anyhow::anyhow!(
                "Assignment failed or access changed; outcome saved"
            )),
            Err(_) => Err(anyhow::anyhow!(
                "Assignment completion unconfirmed; durable reconciliation required"
            )),
        }
    };
    execution_cancel.cancel();
    watchdog.abort();
    let _ = watchdog.await;
    tracing::info!(assignment=%request.scope.assignment,success=outcome.is_ok(),"Controller-scoped worker exiting");
    outcome
}
