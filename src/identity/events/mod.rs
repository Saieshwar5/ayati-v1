pub(crate) mod catalog;
pub(crate) mod model;
pub(crate) mod provider_events;
pub(crate) mod provider_intake;
mod provider_routes;
#[cfg(test)]
mod provider_routes_tests;
pub(super) mod routes;
pub(crate) mod subscriptions;
pub(crate) mod system_events;
mod system_routes;
#[cfg(test)]
pub(crate) mod test_catalog;
pub(crate) mod webhook;
pub(super) fn routes() -> axum::Router<super::Shared> {
    routes::routes()
        .merge(provider_routes::routes())
        .merge(system_routes::routes())
}

#[cfg(test)]
mod tests;
