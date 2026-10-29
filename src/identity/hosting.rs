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
pub(super) async fn ready(State(app): State<Shared>) -> StatusCode {
    let probe = app.store.ping();
    if matches!(
        tokio::time::timeout(std::time::Duration::from_secs(3), probe).await,
        Ok(Ok(()))
    ) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
