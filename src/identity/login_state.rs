//! PostgreSQL-backed, single-use sign-in state survives API replacement.
use super::*;
pub(super) async fn save(app: &App, hash: &str, flow: Pending) -> Result<()> {
    app.store.save_login_flow(hash, &flow).await
}
pub(super) async fn take(app: &App, hash: &str) -> Result<Pending> {
    app.store
        .take_login_flow(hash)
        .await?
        .context("Sign-in expired")
}
