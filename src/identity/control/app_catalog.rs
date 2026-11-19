//! Add app (connector screens 2 and 3): the catalog every member can browse, and
//! the one-step connect admins use. Neither names a provider.
use super::*;
use crate::identity::connectors;
use crate::identity::postgres::connector_routing::{CatalogQuery, Offer};
use axum::extract::Query;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/workspaces/{w}/apps/catalog", get(catalog))
        .route("/api/identity/workspaces/{w}/apps/connect", post(connect))
        .route(
            "/api/identity/workspaces/{w}/apps/allowed",
            get(allowed).post(allow),
        )
}

/// The catalog apps this workspace can connect now, each through its route.
async fn offers(store: &crate::identity::postgres::PgStore, w: &str) -> Result<Vec<Offer>> {
    let added = store.added_apps(w).await?;
    Ok(connectors::apps(store, w)
        .await?
        .into_iter()
        .map(|app| {
            let operations: Value = app
                .operations
                .iter()
                .map(|o| json!({"id":o.id,"title":o.title,"tool":o.tool,"effect":o.effect}))
                .collect();
            // Apps outside Cuckoo's catalog are the operator's own MCP servers.
            let about = connectors::curated::app(&app.id)
                .map(|c| (c.description.clone(), c.category.clone()))
                .or_else(|| {
                    let added = added.iter().find(|a| a.id == app.id)?;
                    Some((added.description.clone(), added.category.clone()))
                });
            let (description, category) = match about {
                Some(about) => about,
                None => (
                    app.account
                        .clone()
                        .unwrap_or_else(|| "Set up by your operator".into()),
                    "company".into(),
                ),
            };
            Offer {
                id: app.id,
                name: app.name,
                description,
                category,
                operations,
            }
        })
        .collect())
}

async fn catalog(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(input): Query<CatalogQuery>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let offered = offers(&app.store, &w).await?;
    Ok(Json(
        app.store
            .connector_catalog(&user, &w, &offered, &input)
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectApp {
    app: String,
    /// The client's id for this click: a repeat returns the same answer.
    request: String,
    /// Connect it for this agent, as its own (schema 132). Without it the
    /// connection is the workspace's and only an admin may make it.
    #[serde(default)]
    agent: Option<String>,
}
/// Approves the app and starts connecting it: `{redirect_url}` for the app's
/// sign-in page, or `{connection}` when verified at once.
async fn connect(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<ConnectApp>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    // Every connection belongs to an agent (schema 133).
    check(
        input.agent.is_some(),
        "Connect apps on an agent's Connections tab",
    )?;
    check(
        execution_available(&app)?,
        "Business tools need controller-managed execution; ask your operator to enable it",
    )?;
    let provider = connectors::provider_for(&app.store, &w, &input.app).await?;
    Ok(Json(
        connectors::connect::connect_app_for(
            &app.store,
            &app.config.origin,
            &user,
            &w,
            &input.app,
            &input.request,
            input.agent.as_deref(),
            provider.as_ref(),
        )
        .await?,
    ))
}

/// Settings › Connections for admins: every app Cuckoo offers, with the
/// admins' decision about it (schema 134).
async fn allowed(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let offered = offers(&app.store, &w).await?;
    let mut listed = app.store.allowed_apps(&user, &w, &offered).await?;
    // Apps the admins added carry their review, so Settings can change or remove them.
    let facts = app.store.added_apps_facts(&w).await?;
    if let Some(apps) = listed["apps"].as_array_mut() {
        for item in apps {
            let fact = item["id"].as_str().and_then(|id| facts.get(id)).cloned();
            item["added"] = fact.unwrap_or(Value::Null);
        }
    }
    listed["library"] = json!(connectors::library().is_some());
    Ok(Json(listed))
}
/// An admin allows an app for the workspace, or switches it off with a note.
/// Switching off disconnects every agent's connection of it.
async fn allow(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<crate::identity::postgres::allowed_apps::AppAllowance>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(
        app.store.role(&w, &user.id).await? == "Admin",
        "Only admins choose the workspace's apps",
    )?;
    if input.allowed {
        connectors::provider_for(&app.store, &w, &input.app).await?;
    }
    let approval = connections_model::SystemApproval {
        server: input.app.clone(),
        approved: input.allowed,
        revision: input.revision,
    };
    let released = app
        .store
        .control_system_noted(&user, &w, &approval, &input.note)
        .await?;
    let revoked = connections::release_all(&w, released).await;
    Ok(Json(json!({"saved":true,"provider_revoked":revoked})))
}
