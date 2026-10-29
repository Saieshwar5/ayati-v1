//! Explicit public origin behind a TLS proxy; forwarded headers never establish trust.
use super::*;

pub(super) fn hosted(app: &App) -> bool {
    app.config.hosted()
}
/// Operator-declared executor capability, not a live availability probe.
/// Provider keys live only on executors.
pub(super) fn provider_configured(app: &App, provider: &str) -> bool {
    app.config.worker_providers.iter().any(|p| p == provider)
}
pub(super) async fn live() -> StatusCode {
    StatusCode::OK
}
/// Ready for new traffic: not stopping and the database answers. The body names
/// the reason so operators can tell a planned drain from an outage.
pub(super) async fn ready(State(app): State<Shared>) -> (StatusCode, &'static str) {
    if app.runtimes.draining.is_cancelled() {
        return (StatusCode::SERVICE_UNAVAILABLE, "draining");
    }
    let probe = app.store.ping();
    if matches!(
        tokio::time::timeout(std::time::Duration::from_secs(3), probe).await,
        Ok(Ok(()))
    ) {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
    }
}
