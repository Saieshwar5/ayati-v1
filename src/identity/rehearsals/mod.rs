//! Personal draft-only model samples, distinct from business task execution.
pub(crate) mod model;
mod routes;
mod worker;
pub(crate) use worker::start;
pub(super) fn routes() -> axum::Router<super::Shared> {
    routes::routes()
}
