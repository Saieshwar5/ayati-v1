//! Where a provider's sign-in page returns, and Cuckoo's own switch per
//! provider. Connecting itself starts on the workspace Connections endpoint.
use super::{ConnectorProvider, named};
use crate::identity::control::connections_model::WorkspaceConnection;
use crate::identity::postgres::{
    PgStore,
    connector_providers::SignIn,
    workspace_connections::{Released, Verified},
};
use crate::identity::*;
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

pub(in crate::identity) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/connectors/return/{id}", get(returned))
        .route(
            "/api/identity/admin/connector-providers",
            get(providers).post(switch),
        )
}

/// The admin is back from the provider's sign-in page. The query string the
/// provider adds is ignored: the account is checked with the provider itself.
async fn returned(
    State(app): State<Shared>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Ok(user) = identity(&app, &headers).await else {
        return Redirect::to("/").into_response();
    };
    let Ok(sign_in) = app.store.connector_sign_in(&user, &id).await else {
        return Redirect::to("/").into_response();
    };
    let page = format!(
        "/w/{}/?view=settings&section=connections",
        sign_in.workspace
    );
    let finished = match named(&sign_in.provider) {
        Ok(provider) => finish_sign_in(&app.store, &user, &id, sign_in, provider.as_ref()).await,
        Err(error) => Err(error),
    };
    match finished {
        Ok(()) => Redirect::to(&format!("{page}&connected=1")).into_response(),
        Err(error) => {
            tracing::warn!("A connector sign-in did not finish: {error}");
            Redirect::to(&format!("{page}&connect_error=1")).into_response()
        }
    }
}

/// Completes a sign-in: the provider confirms the account and Cuckoo saves it
/// like any verified connection. A failed sign-in is removed at the provider.
pub(crate) async fn finish_sign_in(
    store: &PgStore,
    user: &User,
    id: &str,
    sign_in: SignIn,
    provider: &dyn ConnectorProvider,
) -> anyhow::Result<()> {
    let w = sign_in.workspace.clone();
    let saved = async {
        anyhow::ensure!(
            store
                .connector_provider_paused(provider.name())
                .await?
                .is_none(),
            "Cuckoo paused this connector provider"
        );
        let account = tokio::time::timeout(
            Duration::from_secs(60),
            provider.finish(&w, &sign_in.server, &sign_in.binding),
        )
        .await
        .map_err(|_| anyhow::anyhow!("The provider did not confirm the account in time"))??;
        let input = WorkspaceConnection {
            id: sign_in.connection.clone(),
            server: sign_in.server.clone(),
            enabled: true,
            revision: sign_in.revision,
        };
        let verified = Verified {
            definition: account.definition,
            fingerprint: account.fingerprint,
            provider: provider.name().into(),
            binding: account.binding,
        };
        store
            .workspace_connection_bind(user, &w, &input, Some(&verified))
            .await
    }
    .await;
    match saved {
        Ok(released) => {
            store.connector_sign_in_done(id, true).await?;
            match released {
                Some(old) if old.provider == provider.name() => {
                    if provider
                        .disconnect(&w, &old.server, Some(&old.binding))
                        .await
                        .is_err()
                    {
                        tracing::warn!(
                            "The {} provider did not confirm revoking a replaced account",
                            old.provider
                        );
                    }
                }
                other => {
                    release(&w, other).await;
                }
            }
            Ok(())
        }
        Err(error) => {
            store.connector_sign_in_done(id, false).await?;
            let _ = provider
                .disconnect(&w, &sign_in.server, Some(&sign_in.binding))
                .await;
            Err(error)
        }
    }
}

/// Asks the provider to revoke an account Cuckoo stopped using. Cuckoo already
/// stopped using it, so a failure here is reported, not fatal.
pub(crate) async fn release(w: &str, released: Option<Released>) -> bool {
    let Some(released) = released else {
        return true;
    };
    let done = match named(&released.provider) {
        Ok(provider) => tokio::time::timeout(
            Duration::from_secs(20),
            provider.disconnect(w, &released.server, Some(&released.binding)),
        )
        .await
        .is_ok_and(|r| r.is_ok()),
        Err(_) => false,
    };
    if !done {
        tracing::warn!(
            "The {} provider did not confirm revoking a disconnected {} account",
            released.provider,
            released.server
        );
    }
    done
}

async fn providers(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    platform(&app, &headers).await?;
    let mut value = app.store.connector_providers().await?;
    value["configured"] = json!(
        super::providers()
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>()
    );
    Ok(Json(value))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Switch {
    provider: String,
    enabled: bool,
    #[serde(default)]
    note: String,
}
/// Switches one provider off for every workspace, or back on.
async fn switch(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Switch>,
) -> ApiResult<Json<Value>> {
    let user = platform(&app, &headers).await?;
    app.store
        .connector_provider_set(&user, &input.provider, input.enabled, &input.note)
        .await?;
    Ok(Json(json!({"saved":true})))
}

async fn platform(app: &App, headers: &HeaderMap) -> ApiResult<User> {
    let user = identity(app, headers).await?;
    if !workspace_codes::platform_admin(app, &user) {
        return Err(Error(
            anyhow::anyhow!("Only Cuckoo administrators can switch connector providers"),
            axum::http::StatusCode::FORBIDDEN,
        ));
    }
    Ok(user)
}
