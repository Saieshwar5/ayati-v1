//! Add more apps (schema 135): a workspace's admins search a provider's library,
//! review one app's actions and add it. Everything saved is read from the
//! provider on the server; the browser only names the app, its tools and
//! whether each reads or changes.
use super::*;
use crate::identity::connectors::{self, LibraryPick};
use axum::extract::Query;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/workspaces/{w}/apps/library", get(search))
        .route(
            "/api/identity/workspaces/{w}/apps/library/{app}/actions",
            get(actions),
        )
        .route("/api/identity/workspaces/{w}/apps/added", post(save))
        .route(
            "/api/identity/workspaces/{w}/apps/added/{app}/remove",
            post(remove),
        )
}

/// The admin and the library's provider, or a refusal.
async fn admin(
    app: &Shared,
    headers: &HeaderMap,
    w: &str,
) -> ApiResult<(
    crate::identity::user::User,
    std::sync::Arc<dyn connectors::ConnectorProvider>,
)> {
    let user = identity(app, headers).await?;
    check(
        app.store.role(w, &user.id).await? == "Admin",
        "Only admins add apps to the workspace",
    )?;
    let Some(provider) = connectors::library() else {
        return Err(
            anyhow::anyhow!("No app library is set up on this server; ask your operator").into(),
        );
    };
    Ok((user, provider))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    #[serde(default)]
    q: String,
    #[serde(default)]
    page: Option<String>,
}

/// Library apps matching the search, each marked addable, built in, already
/// added, or not supported with the reason.
async fn search(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(input): Query<Search>,
) -> ApiResult<Json<Value>> {
    let (_, provider) = admin(&app, &headers, &w).await?;
    let mut found = provider.library(&input.q).await?;
    let added = app.store.added_apps(&w).await?;
    if let Some(apps) = found["apps"].as_array_mut() {
        for listed in apps {
            if added.iter().any(|a| listed["id"] == a.id.as_str()) {
                listed["state"] = json!("added");
            }
        }
    }
    Ok(Json(found))
}

/// One page of a library app's actions to review, with the admin's saved
/// choices when the app was added before.
async fn actions(
    State(app): State<Shared>,
    RoutePath((w, name)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Query(input): Query<Search>,
) -> ApiResult<Json<Value>> {
    let (_, provider) = admin(&app, &headers, &w).await?;
    let mut found = provider
        .library_actions(&name, &input.q, input.page.as_deref())
        .await?;
    let facts = app.store.added_apps_facts(&w).await?;
    let id = found["id"].as_str().unwrap_or_default().to_owned();
    found["added"] = facts.get(&id).cloned().unwrap_or(Value::Null);
    Ok(Json(found))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    /// The library's name for the app.
    app: String,
    actions: Vec<LibraryPick>,
    /// 0 to add; the revision shown to change an added app's actions.
    revision: i64,
}

/// Adds the app with the chosen actions and allows it, or changes the actions
/// of an added app, which ends every connection of it.
async fn save(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<Save>,
) -> ApiResult<Json<Value>> {
    let (user, provider) = admin(&app, &headers, &w).await?;
    check(
        (1..=connectors::added::MAX_ACTIONS).contains(&input.actions.len()),
        "Choose 1 to 20 actions",
    )?;
    let added = provider.library_app(&input.app, &input.actions).await?;
    // The name must not be an app this workspace is already offered another way.
    if input.revision == 0 {
        let offered = connectors::apps(&app.store, &w).await?;
        check(
            !offered.iter().any(|o| o.id == added.id),
            "This app is already offered in this workspace",
        )?;
    }
    let released = app
        .store
        .added_app_save(&user, &w, &added, input.revision)
        .await?;
    let ended = released.len();
    let revoked = connections::release_all(&w, released).await;
    Ok(Json(
        json!({"saved":true,"app":added.id,"name":added.name,"ended":ended,"provider_revoked":revoked}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Remove {
    revision: i64,
}

/// Removes an added app: every connection of it ends and it is no longer offered.
async fn remove(
    State(app): State<Shared>,
    RoutePath((w, name)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Remove>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let released = app
        .store
        .added_app_remove(&user, &w, &name, input.revision)
        .await?;
    let ended = released.len();
    let revoked = connections::release_all(&w, released).await;
    Ok(Json(
        json!({"removed":true,"ended":ended,"provider_revoked":revoked}),
    ))
}
