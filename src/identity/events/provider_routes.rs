//! Provider event routes (schema 109): one intake per provider, the platform
//! admin's Composio project webhook, and a connected app's events.
use super::super::*;
use super::{provider_intake, subscriptions};
use crate::identity::connectors::{self, composio_triggers};
use crate::identity::secrets::Sealer;
use axum::{
    Json,
    body::Bytes,
    extract::{DefaultBodyLimit, Path as RoutePath},
    http::HeaderMap,
};
use serde_json::{Value, json};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/hooks/composio",
            post(composio_hook).layer(DefaultBodyLimit::max(64000)),
        )
        .route(
            "/hooks/pipedream/{subscription}",
            post(pipedream_hook).layer(DefaultBodyLimit::max(64000)),
        )
        .route(
            "/api/identity/admin/connector-events/composio",
            get(composio_setup).post(set_up_composio),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/{connection}/events",
            get(app_events),
        )
}

fn sealer(app: &App) -> Option<Sealer> {
    app.config.secrets_key_file.as_deref().map(Sealer::new)
}

async fn composio_hook(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    let sealed = app
        .store
        .connector_event_secret("composio")
        .await
        .map_err(|_| provider_intake::unavailable())?
        .ok_or_else(provider_intake::unavailable)?;
    let secret = sealer(&app)
        .ok_or_else(provider_intake::unavailable)?
        .open(provider_intake::COMPOSIO_SECRET, &sealed)
        .map_err(|_| provider_intake::unavailable())?;
    let provider = connectors::named("composio").map_err(|_| provider_intake::unavailable())?;
    let limit = app.config.event_receipt_limit;
    provider_intake::composio(
        &app.store,
        provider.as_ref(),
        &headers,
        &body,
        &secret,
        limit,
    )
    .await
    .map(Json)
}

/// Pipedream's own package parses its deliveries; without it this is 404.
async fn pipedream_hook(
    State(app): State<Shared>,
    RoutePath(subscription): RoutePath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    let provider = connectors::named("pipedream").map_err(|_| {
        Error(
            anyhow::anyhow!("Unknown event subscription"),
            StatusCode::NOT_FOUND,
        )
    })?;
    let sealer = sealer(&app);
    let limit = app.config.event_receipt_limit;
    provider_intake::signed(
        &app.store,
        provider.as_ref(),
        sealer.as_ref(),
        &subscription,
        &headers,
        &body,
        limit,
    )
    .await
    .map(Json)
}

async fn platform(app: &App, headers: &HeaderMap) -> ApiResult<User> {
    let user = identity(app, headers).await?;
    if !workspace_codes::platform_admin(app, &user) {
        return Err(Error(
            anyhow::anyhow!("Only Cuckoo administrators can set up app events"),
            StatusCode::FORBIDDEN,
        ));
    }
    Ok(user)
}

/// Whether Composio delivers app events here, never the secret.
async fn setup_state(app: &App) -> ApiResult<Value> {
    let saved = app.store.connector_event_setup("composio").await?;
    Ok(
        json!({"provider":"composio","set_up":saved.is_some(),"setup":saved,
        "webhook_url":app.config.hosted().then(|| format!("{}/hooks/composio", app.config.origin)),
        "public_origin":app.config.hosted(),"secrets_key":app.config.secrets_key_file.is_some(),
        "configured":connectors::named("composio").is_ok()}),
    )
}

async fn composio_setup(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    platform(&app, &headers).await?;
    Ok(Json(setup_state(&app).await?))
}

/// Registers this server's `/hooks/composio` as the Composio project's one
/// webhook for trigger messages and account expiry, and keeps its signing
/// secret sealed in PostgreSQL.
async fn set_up_composio(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let user = platform(&app, &headers).await?;
    check(
        app.config.hosted(),
        "Set AYATI_PUBLIC_ORIGIN to this server's public HTTPS address first: Composio delivers app events there.",
    )?;
    let sealer = sealer(&app).ok_or_else(|| {
        Error(
            anyhow::anyhow!(
                "Set AYATI_SECRETS_KEY_FILE first: Composio's signing secret is kept sealed."
            ),
            StatusCode::BAD_REQUEST,
        )
    })?;
    let url = format!("{}/hooks/composio", app.config.origin);
    let hook = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        composio_triggers::register_webhook(&app.config, &url),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Composio did not answer in time; try again"))??;
    let sealed = sealer.seal(provider_intake::COMPOSIO_SECRET, &hook.secret)?;
    app.store
        .connector_event_setup_save(
            &user,
            "composio",
            &hook.id,
            &url,
            &composio_triggers::EVENTS,
            &sealed,
        )
        .await?;
    let by = crate::identity::postgres::operator::Operator {
        user,
        via: crate::identity::postgres::operator::Via::Web,
    };
    app.store
        .operator_record(
            &by,
            "providers",
            "connector_events_set_up",
            "composio",
            "pointed Composio's project webhook at this server for app events",
            "Set up app events",
        )
        .await?;
    let mut value = setup_state(&app).await?;
    value["saved"] = json!(true);
    Ok(Json(value))
}

async fn app_events(
    State(app): State<Shared>,
    RoutePath((w, connection)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.app_events(&user, &w, &connection).await?))
}

/// What creating a trigger from an app needs from this server.
pub(super) async fn context(app: &App) -> subscriptions::Context<'_> {
    let mut unavailable = std::collections::BTreeMap::new();
    for provider in ["composio", "pipedream"] {
        if let Some(why) =
            subscriptions::unavailable(&app.store, &app.config, &connectors::named, provider).await
        {
            unavailable.insert(provider.to_owned(), why);
        }
    }
    subscriptions::Context {
        store: &app.store,
        origin: &app.config.origin,
        sealer: sealer(app),
        providers: &connectors::named,
        unavailable,
    }
}
