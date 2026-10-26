pub(crate) mod model;
mod routes;
mod scheduler;
pub(super) use scheduler::start;
pub(super) fn routes() -> axum::Router<super::Shared> {
    routes::routes()
}
