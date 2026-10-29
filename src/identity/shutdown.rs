//! Graceful stop on Ctrl-C or SIGTERM (container and ECS task stop).
//!
//! A stopping controller first reports itself not ready and keeps serving for
//! `AYATI_DRAIN_SECONDS`, so a load balancer stops routing to it before the port
//! closes. It then stops claiming work, closes streams and waits for running
//! work, up to `AYATI_SHUTDOWN_SECONDS` after the signal. Work still running at
//! that limit is abandoned to the existing lease recovery, as after a crash.
use super::*;
use std::time::Duration;
use tokio::task::JoinHandle;

pub(super) async fn signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = terminate.recv() => {},
    }
}

/// Serve the controller until a stop signal, then drain within the limit.
pub(super) async fn serve(
    app: Shared,
    listener: tokio::net::TcpListener,
    control: Option<JoinHandle<()>>,
) -> Result<()> {
    let draining = app.runtimes.draining.clone();
    let stopping = app.runtimes.stopping.clone();
    let drain = Duration::from_secs(app.config.drain_seconds);
    let limit = Duration::from_secs(app.config.shutdown_seconds);
    let server = axum::serve(listener, router(app.clone())).with_graceful_shutdown({
        let draining = draining.clone();
        async move {
            // An internal failure (stopping) skips the delay: the process is unhealthy.
            tokio::select! { _ = signal() => {}, _ = stopping.cancelled() => {} }
            draining.cancel();
            if !stopping.is_cancelled() && !drain.is_zero() {
                tracing::info!(
                    seconds = drain.as_secs(),
                    "Stop requested; reporting not ready while traffic moves away"
                );
                tokio::select! { _ = tokio::time::sleep(drain) => {}, _ = stopping.cancelled() => {} }
            }
            tracing::info!("Draining: no new work is claimed and streams close");
            stopping.cancel();
        }
    });
    let finished = async {
        server.await?;
        app.runtimes.shutdown().await?;
        if let Some(control) = control {
            control.await?;
        }
        Ok::<_, anyhow::Error>(())
    };
    let deadline = async {
        draining.cancelled().await;
        tokio::time::sleep(limit).await;
    };
    tokio::select! {
        result = finished => {
            tracing::info!("Controller stopped cleanly");
            result
        }
        _ = deadline => anyhow::bail!(
            "Shutdown exceeded AYATI_SHUTDOWN_SECONDS; unfinished work is left to lease recovery"
        ),
    }
}
