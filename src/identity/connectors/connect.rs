//! Starting a connection, from Apps' one-step connect or the older Connections
//! endpoint: verified at once, or the provider's sign-in page to open. The
//! sign-in finishes in [`super::routes::finish_sign_in`].
use super::{Connect, ConnectorProvider};
use crate::identity::control::connections_model::WorkspaceConnection;
use crate::identity::postgres::{
    PgStore, connector_providers::StartSignIn, connector_routing::Begun,
    workspace_connections::Verified,
};
use crate::identity::user::User;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::time::Duration;

pub(crate) enum Started {
    /// Verified at once and saved.
    Connected,
    /// The admin signs in at this page; the provider sends them back to Cuckoo.
    SignIn(String),
}

/// Connects `input`'s account through `provider`. The same checks apply as
/// for any connection: an admin, an approved app and an unchanged connection.
pub(crate) async fn start(
    store: &PgStore,
    origin: &str,
    user: &User,
    w: &str,
    input: &WorkspaceConnection,
    provider: &dyn ConnectorProvider,
) -> Result<Started> {
    store.connection_preflight(user, w, input).await?;
    ensure!(
        store
            .connector_provider_paused(provider.name())
            .await?
            .is_none(),
        "Cuckoo paused this connector provider; try again later"
    );
    let sign_in = uuid::Uuid::new_v4().to_string();
    let callback = format!("{origin}/api/identity/connectors/return/{sign_in}");
    let started = tokio::time::timeout(
        Duration::from_secs(45),
        provider.connect(w, &input.server, &callback),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Connecting timed out. Try again."))??;
    match started {
        Connect::Ready(account) => {
            let verified = Verified {
                definition: account.definition,
                fingerprint: account.fingerprint,
                provider: provider.name().into(),
                binding: account.binding,
                identity: account.identity,
            };
            let released = store
                .workspace_connection_bind(user, w, input, Some(&verified))
                .await?;
            super::routes::release(w, released).await;
            Ok(Started::Connected)
        }
        Connect::SignIn {
            binding,
            url,
            expires,
        } => {
            let start = StartSignIn {
                id: &sign_in,
                provider: provider.name(),
                binding: &binding,
                expires,
            };
            if let Err(error) = store.connector_sign_in_start(user, w, input, &start).await {
                let _ = provider.disconnect(w, &input.server, Some(&binding)).await;
                return Err(error);
            }
            Ok(Started::SignIn(url))
        }
    }
}

/// One-step connect from Add app: approves `app`, then starts connecting it
/// with the safe default rules (reads allowed, deletes blocked, everything else
/// asks first). The same `request` returns the same answer: `{redirect_url}` or
/// `{connection}`. A failed attempt saves no answer, so trying again runs again.
pub(crate) async fn connect_app(
    store: &PgStore,
    origin: &str,
    user: &User,
    w: &str,
    app: &str,
    request: &str,
    provider: &dyn ConnectorProvider,
) -> Result<Value> {
    let (connection, revision) = match store.connector_connect_begin(user, w, request, app).await? {
        Begun::Answered(answer) => return Ok(answer),
        Begun::Start {
            connection,
            revision,
        } => (connection, revision),
    };
    let input = WorkspaceConnection {
        id: connection.clone(),
        server: app.into(),
        enabled: true,
        revision,
    };
    let answer = match start(store, origin, user, w, &input, provider).await {
        Ok(Started::Connected) => json!({"connection": connection}),
        Ok(Started::SignIn(url)) => json!({"redirect_url": url}),
        Err(error) => {
            if store
                .connector_connect_done(w, request, None)
                .await
                .is_err()
            {
                tracing::warn!("A failed connect request could not be closed");
            }
            return Err(error);
        }
    };
    store
        .connector_connect_done(w, request, Some(&answer))
        .await?;
    Ok(answer)
}
