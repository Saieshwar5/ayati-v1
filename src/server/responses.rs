use super::*;
use axum::extract::Query;
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Read {
    // The existing local middleware selects/authenticates the agent before this handler.
    #[serde(rename = "agent")]
    _agent: Option<String>,
    #[serde(default)]
    pub metadata: bool,
    #[serde(default)]
    pub download: bool,
    pub version: Option<String>,
}
pub(super) async fn read(
    axum::Extension(state): axum::Extension<WebState>,
    Path(id): Path<String>,
    Query(input): Query<Read>,
) -> ApiResult<Response> {
    let message = state.agent.store.response_message(&id)?;
    let data = crate::response::body(
        &id,
        &message.content,
        &message.state,
        message.content.chars().count() as i64,
        input.metadata,
    )?;
    Ok(deliver(&id, data, &input))
}
pub(crate) fn deliver(id: &str, data: Value, input: &Read) -> Response {
    if input
        .version
        .as_ref()
        .is_some_and(|version| data["version"] != *version)
    {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error":"This saved version changed. Open the latest saved response."})),
        )
            .into_response();
    }
    let mut response = if input.download {
        if data["state"] != "complete" || data["body"].as_str().is_none() {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error":"Open a complete saved response before downloading."})),
            )
                .into_response();
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            "text/markdown; charset=utf-8".parse().unwrap(),
        );
        let name = crate::response::checksum(id.as_bytes());
        headers.insert(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"response-{}.md\"", &name[..12])
                .parse()
                .unwrap(),
        );
        (headers, data["body"].as_str().unwrap().to_owned()).into_response()
    } else {
        Json(data).into_response()
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
}
