//! PostgreSQL-backed, single-use sign-in state survives API replacement.
use super::*;
pub(super) async fn save(app: &App, hash: &str, flow: Pending) -> Result<()> {
    if let database::Database::Postgres(pg) = &app.store {
        return pg.save_login_flow(hash, &flow).await;
    }
    let mut pending = app.pending.lock().unwrap();
    pending.retain(|_, v| v.expires > now());
    check(
        pending.len() < 100,
        "Too many sign-in attempts. Try again later",
    )?;
    pending.insert(hash.into(), flow);
    Ok(())
}
pub(super) async fn take(app: &App, hash: &str) -> Result<Pending> {
    let flow = if let database::Database::Postgres(pg) = &app.store {
        pg.take_login_flow(hash).await?
    } else {
        app.pending.lock().unwrap().remove(hash)
    };
    flow.context("Sign-in expired")
}
