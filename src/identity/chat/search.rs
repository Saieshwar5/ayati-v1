//! `GET /api/identity/workspaces/{workspace}/message-search`: messages the person
//! can read, found by their words (`postgres::message_search`).
use super::*;
use crate::identity::postgres::message_search::SearchInput;
use axum::extract::Query;

pub(super) fn routes() -> Router<Shared> {
    Router::new().route(
        "/api/identity/workspaces/{workspace}/message-search",
        get(search),
    )
}

async fn search(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Query(input): Query<SearchInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.search_messages(&user, &w, &input).await?))
}
