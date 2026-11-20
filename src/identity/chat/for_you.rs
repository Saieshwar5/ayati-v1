//! The route for For you (`postgres/for_you.rs`).
use super::*;
use axum::extract::Query;

pub(super) fn routes() -> Router<Shared> {
    Router::new().route("/api/identity/workspaces/{w}/for-you", get(list))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    before: Option<String>,
}

async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Query(page): Query<Page>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.for_you(&user, &w, page.before.as_deref()).await?,
    ))
}
