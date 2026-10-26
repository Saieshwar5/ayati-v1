pub(crate) mod model;
#[cfg(test)]
mod model_tests;
use super::*;
use axum::{
    Json,
    extract::{DefaultBodyLimit, Path},
    http::HeaderMap,
};
use serde_json::Value;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/questions/{id}",
            get(read).post(answer),
        )
        .layer(DefaultBodyLimit::max(12288))
}
async fn read(
    State(app): State<Shared>,
    Path((w, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        collaboration::pg(&app)?.question(&user, &w, &id).await?,
    ))
}
async fn answer(
    State(app): State<Shared>,
    Path((w, id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<model::Submission>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        collaboration::pg(&app)?
            .answer_question(&user, &w, &id, &input)
            .await?,
    ))
}
