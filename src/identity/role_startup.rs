//! Starts the parts of the controller this process's role runs
//! (runtime/controller-roles.md). Nothing here links the roles in memory:
//! each part reads its work from PostgreSQL and is woken by its notifications.
use super::*;

/// Returns worker control's task when this process serves executors.
pub(super) async fn start(app: &Shared) -> Result<Option<tokio::task::JoinHandle<()>>> {
    let role = app.config.role;
    tracing::info!(role = role.name(), "Controller role");
    let control = if role.work() {
        worker_control::start(app.clone()).await?
    } else {
        None
    };
    // Every role listens to PostgreSQL: live streams need message changes,
    // background loops need work wake-ups.
    chat::start_dispatcher(app.clone());
    metrics::start(app.clone());
    if role.work() {
        automations::start(app.clone()).await;
        communications::gateway::start(app.clone());
        control::outcome_checks::start(app.clone());
        data_cleanup::start(app.clone());
        connectors::drift::start(app.clone());
        library::indexer::start(app.clone());
        context_compaction::start(app.clone());
        agent_learning::start(app.clone());
        memory_tidy::start(app.clone());
        evolution::start(app.clone());
        person_asks::start(app.clone());
        library::keeping::start(app.clone());
        computers::sleeper::start(app.clone());
        transcription::start(app.clone())?;
    }
    Ok(control)
}

/// A work controller serves no pages: its browser port answers health checks only.
pub(super) fn health_router(app: Shared) -> Router {
    Router::new()
        .route("/health/live", get(hosting::live))
        .route("/health/ready", get(hosting::ready))
        .with_state(app)
}
