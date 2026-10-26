pub(crate) mod catalog;
pub(crate) mod model;
pub(super) mod routes;
#[cfg(test)]
pub(crate) mod test_catalog;
pub(super) fn routes() -> axum::Router<super::Shared> {
    routes::routes()
}

#[cfg(test)]
mod tests;
