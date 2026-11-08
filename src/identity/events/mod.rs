pub(crate) mod catalog;
pub(crate) mod model;
pub(crate) mod provider_events;
pub(super) mod routes;
#[cfg(test)]
pub(crate) mod test_catalog;
pub(crate) mod webhook;
pub(super) fn routes() -> axum::Router<super::Shared> {
    routes::routes()
}

#[cfg(test)]
mod tests;
