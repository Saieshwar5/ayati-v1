//! A conversation's files for the people in it, an agent's artifacts across its
//! conversations, and keeping one in the Library (schema 159).
use super::*;
use crate::identity::postgres::conversation_files::KeepFile;

pub(super) async fn listed(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.chat_files(&user, &w, &chat).await?))
}
pub(super) async fn artifacts(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.agent_artifacts(&user, &w, &agent).await?))
}
pub(super) async fn keep(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<KeepFile>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.chat_file_keep(&user, &w, &id, &input).await?,
    ))
}
