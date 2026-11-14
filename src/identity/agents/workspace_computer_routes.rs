//! A workspace's own computers (B3, D248). Admins add them in Settings ›
//! Computers; the Cuckoo Connector on each machine signs in once with a
//! one-time code and then asks for work under `/hooks/computers/{id}/…` with
//! its key. No cookie authorizes those hooks. See runtime/workspace-computers.md.
use super::*;
use crate::identity::postgres::shared_computers::Refused;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/workspace-computers",
            get(list).post(add),
        )
        .route(
            "/api/identity/workspaces/{w}/workspace-computers/{id}/code",
            post(new_code),
        )
        .route(
            "/api/identity/workspaces/{w}/workspace-computers/{id}/agents",
            post(choose),
        )
        .route(
            "/api/identity/workspaces/{w}/workspace-computers/{id}/remove",
            post(remove),
        )
        .route("/hooks/computers/{id}/sign-in", post(sign_in))
        .route("/hooks/computers/{id}/next", post(next))
        .route(
            "/hooks/computers/{id}/done",
            post(done).layer(DefaultBodyLimit::max(1_500_000)),
        )
}

async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.workspace_computers(&user, &w).await?))
}

#[derive(serde::Deserialize)]
struct Add {
    id: String,
    name: String,
}

async fn add(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(body): Json<Add>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .add_workspace_computer(&user, &w, &body.id, &body.name)
            .await?,
    ))
}

#[derive(serde::Deserialize)]
struct Revision {
    revision: i64,
    #[serde(default)]
    agents: Vec<String>,
}

async fn new_code(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Revision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .new_computer_code(&user, &w, &id, body.revision)
            .await?,
    ))
}

async fn choose(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Revision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .choose_computer_agents(&user, &w, &id, body.revision, &body.agents)
            .await?,
    ))
}

async fn remove(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Revision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .remove_workspace_computer(&user, &w, &id, body.revision)
            .await?,
    ))
}

#[derive(serde::Deserialize)]
struct SignIn {
    code: String,
    #[serde(default)]
    platform: String,
}

async fn sign_in(
    State(app): State<Shared>,
    RoutePath(id): RoutePath<String>,
    Json(body): Json<SignIn>,
) -> ApiResult<Json<Value>> {
    answer(
        app.store
            .machine_sign_in(&id, &body.code, &body.platform)
            .await,
    )
}

/// Long poll: the next piece of work, or `{"call":null}` after about 25 seconds.
async fn next(
    State(app): State<Shared>,
    RoutePath(id): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let key = bearer(&headers)?;
    let wait = std::time::Duration::from_secs(25);
    answer(app.store.machine_next(&id, key, wait).await)
}

#[derive(serde::Deserialize)]
struct Done {
    call: String,
    result: Value,
}

async fn done(
    State(app): State<Shared>,
    RoutePath(id): RoutePath<String>,
    headers: HeaderMap,
    Json(body): Json<Done>,
) -> ApiResult<Json<Value>> {
    let key = bearer(&headers)?;
    answer(
        app.store
            .machine_done(&id, key, &body.call, &body.result)
            .await,
    )
}

fn answer(result: anyhow::Result<Value>) -> ApiResult<Json<Value>> {
    match result {
        Ok(value) => Ok(Json(value)),
        Err(e) if e.is::<Refused>() => Err(refused()),
        Err(e) => Err(Error(e, StatusCode::BAD_REQUEST)),
    }
}

fn refused() -> Error {
    Error(anyhow::anyhow!("{Refused}"), StatusCode::UNAUTHORIZED)
}

fn bearer(headers: &HeaderMap) -> ApiResult<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(refused)
}

/// `/hooks/computers/<uuid>/sign-in|next|done`, for the Origin-check
/// exception. The connector's key or one-time code authorizes them.
pub(crate) fn hook_path(path: &str) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    parts.len() == 5
        && parts[0].is_empty()
        && parts[1..3] == ["hooks", "computers"]
        && uuid::Uuid::parse_str(parts[3]).is_ok()
        && matches!(parts[4], "sign-in" | "next" | "done")
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_connector_hooks_skip_the_origin_check() {
        let id = "6f2b5a30-4c1e-4a8e-9a77-0d2c1f3b9e10";
        for end in ["sign-in", "next", "done"] {
            assert!(super::hook_path(&format!("/hooks/computers/{id}/{end}")));
        }
        for path in [
            format!("/hooks/computers/{id}"),
            format!("/hooks/computers/{id}/remove"),
            "/hooks/computers/not-a-uuid/next".to_owned(),
            format!("/api/hooks/computers/{id}/next"),
            format!("/hooks/computers/{id}/next/more"),
        ] {
            assert!(!super::hook_path(&path), "{path}");
        }
    }
}
