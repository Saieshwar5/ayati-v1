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
}

/// The catalog apps this workspace can connect now, each through its route.
async fn offers(store: &crate::identity::postgres::PgStore, w: &str) -> Result<Vec<Offer>> {
    Ok(connectors::apps(store, w)
        .await?
        .into_iter()
        .filter_map(|app| {
            let catalog = connectors::curated::app(&app.id)?;
            let operations: Value = app
                .operations
                .iter()
                .map(|o| json!({"id":o.id,"title":o.title,"tool":o.tool,"effect":o.effect}))
                .collect();
            Some(Offer {
                id: app.id,
                name: app.name,
                description: catalog.description.clone(),
                category: catalog.category.clone(),
                operations,
            })
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
    check(
        app.store.role(&w, &user.id).await? == "Admin",
        "Only workspace admins can connect apps",
    )?;
    check(
        execution_available(&app)?,
        "Business tools need controller-managed execution; ask your operator to enable it",
    )?;
    let provider = connectors::provider_for(&app.store, &w, &input.app).await?;
    Ok(Json(
        connectors::connect::connect_app(
            &app.store,
            &app.config.origin,
            &user,
            &w,
            &input.app,
            &input.request,
            provider.as_ref(),
        )
        .await?,
    ))
}
