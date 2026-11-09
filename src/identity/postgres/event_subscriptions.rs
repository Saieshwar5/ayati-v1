//! Provider subscriptions as event sources (schema 109): reading them back as
//! [`Source`]s, the project-wide delivery setup, account expiry and provider
//! clean-up. Signing keys and secrets stay sealed here; callers open them.
use super::*;
use crate::identity::events::catalog::{self, Source, Subscribed};
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;
use std::collections::BTreeMap;

#[path = "event_app_triggers.rs"]
pub(crate) mod app_triggers;
#[cfg(test)]
#[path = "event_provider_intake_tests.rs"]
mod intake_tests;
#[cfg(test)]
#[path = "event_subscription_tests.rs"]
mod subscription_tests;

pub(super) struct ProviderEvents;
impl MigrationName for ProviderEvents {
    fn name(&self) -> &str {
        "m20261108_000109_provider_events"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ProviderEvents {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("event_subscriptions.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Provider subscriptions require a forward migration".into(),
        ))
    }
}

/// The columns a subscription source is built from.
pub(super) const COLUMNS: &str = "workspace,id,connection,event,provider,provider_id,trigger,fields,binding,connection_fingerprint,read_operation,name,state";

/// A subscription row as an event source. The same fields always build the
/// same source, so its fingerprint (a trigger's binding) is stable.
pub(super) fn source_from(row: &Value) -> Result<Source> {
    let text = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
    let binding = &row["binding"];
    let map: BTreeMap<String, String> = serde_json::from_value(row["fields"].clone())?;
    let source = Source {
        id: text("id"),
        workspace: text("workspace"),
        connection: text("connection"),
        connection_fingerprint: text("connection_fingerprint"),
        name: text("name"),
        provider: text("provider"),
        user_id: binding["user"].as_str().unwrap_or_default().to_owned(),
        connected_account_id: binding["account"].as_str().unwrap_or_default().to_owned(),
        auth_config_id: String::new(),
        trigger_id: text("provider_id"),
        trigger_slug: text("trigger"),
        read_operation: text("read_operation"),
        fields: map.keys().cloned().collect(),
        secret_file: String::new(),
        subscribed: Some(Subscribed {
            event: text("event"),
            map,
            active: row["state"] == "active",
        }),
    };
    source.validate()?;
    Ok(source)
}

pub(super) async fn row(
    db: &impl ConnectionTrait,
    sql: &str,
    values: Vec<sea_orm::Value>,
) -> Result<Option<Value>> {
    let sql = format!(
        "SELECT jsonb_build_object({}) AS data FROM relay_connector_subscriptions s WHERE {sql}",
        COLUMNS
            .split(',')
            .map(|c| format!("'{c}',s.{c}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    query(db, &sql, values)
        .await?
        .map(|r| r.try_get::<Value>("", "data"))
        .transpose()
        .map_err(Into::into)
}

/// The subscription `id` of workspace `w`, in any state.
pub(super) async fn source(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<Option<Source>> {
    if !exists(db).await? {
        return Ok(None);
    }
    row(db, "s.workspace=$1 AND s.id=$2", vec![w.into(), id.into()])
        .await?
        .map(|r| source_from(&r))
        .transpose()
}

/// Any event source of the workspace: an operator profile first, then a
/// provider subscription. A database error is never mistaken for "missing".
pub(super) async fn any_source(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<Source> {
    if let Ok(source) = catalog::source(w, id) {
        return Ok(source);
    }
    source(db, w, id)
        .await?
        .context("Event source is unavailable")
}

async fn exists(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_connector_subscriptions') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .unwrap()
    .try_get("", "present")?)
}

/// A subscription found from a provider delivery.
pub(crate) struct Hooked {
    pub source: Source,
    /// The sealed per-subscription signing key, if the provider issued one.
    pub signing_key: Option<Vec<u8>>,
}

/// A subscription the provider should stop: removed or broken in Cuckoo.
pub(crate) struct Release {
    pub workspace: String,
    pub id: String,
    pub provider: String,
    pub provider_id: String,
    pub server: String,
    pub binding: Value,
}

impl PgStore {
    /// The subscription a provider names, in any workspace and state.
    pub(crate) async fn subscription_by_provider(
        &self,
        provider: &str,
        provider_id: &str,
    ) -> Result<Option<Hooked>> {
        let sql = "s.provider=$1 AND s.provider_id=$2 ORDER BY (s.state='active') DESC, s.created_ms DESC LIMIT 1";
        self.hooked(row(&self.db, sql, vec![provider.into(), provider_id.into()]).await?)
            .await
    }

    /// The subscription a per-subscription webhook URL names.
    pub(crate) async fn subscription_by_id(
        &self,
        provider: &str,
        id: &str,
    ) -> Result<Option<Hooked>> {
        let sql = "s.provider=$1 AND s.id=$2";
        self.hooked(row(&self.db, sql, vec![provider.into(), id.into()]).await?)
            .await
    }

    async fn hooked(&self, data: Option<Value>) -> Result<Option<Hooked>> {
        let Some(data) = data else { return Ok(None) };
        let source = source_from(&data)?;
        let key = query(
            &self.db,
            "SELECT signing_key FROM relay_connector_subscriptions WHERE workspace=$1 AND id=$2",
            vec![source.workspace.clone().into(), source.id.clone().into()],
        )
        .await?
        .map(|r| r.try_get::<Option<Vec<u8>>>("", "signing_key"))
        .transpose()?
        .flatten();
        Ok(Some(Hooked {
            source,
            signing_key: key,
        }))
    }

    /// Whether `provider`'s project-wide delivery is set up, without its secret.
    pub(crate) async fn connector_event_setup(&self, provider: &str) -> Result<Option<Value>> {
        query(&self.db, "SELECT jsonb_build_object('webhook_url',s.webhook_url,'events',s.events,'changed',s.changed,'changed_by',COALESCE(u.display_name,u.email)) AS data FROM relay_connector_event_setup s LEFT JOIN relay_users u ON u.id=s.changed_by WHERE s.provider=$1", vec![provider.into()])
            .await?
            .map(|r| r.try_get::<Value>("", "data"))
            .transpose()
            .map_err(Into::into)
    }

    /// The sealed project signing secret for `provider`'s deliveries.
    pub(crate) async fn connector_event_secret(&self, provider: &str) -> Result<Option<Vec<u8>>> {
        query(
            &self.db,
            "SELECT secret FROM relay_connector_event_setup WHERE provider=$1",
            vec![provider.into()],
        )
        .await?
        .map(|r| r.try_get::<Vec<u8>>("", "secret"))
        .transpose()
        .map_err(Into::into)
    }

    pub(crate) async fn connector_event_setup_save(
        &self,
        by: &User,
        provider: &str,
        provider_id: &str,
        webhook_url: &str,
        events: &[&str],
        sealed: &[u8],
    ) -> Result<()> {
        execute(&self.db, "INSERT INTO relay_connector_event_setup(provider,provider_id,webhook_url,events,secret,changed_by,changed) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(provider) DO UPDATE SET provider_id=excluded.provider_id,webhook_url=excluded.webhook_url,events=excluded.events,secret=excluded.secret,changed_by=excluded.changed_by,changed=excluded.changed",
            vec![provider.into(), provider_id.into(), webhook_url.into(), json!(events).into(), sealed.to_vec().into(), by.id.clone().into(), crate::identity::user::now().into()]).await?;
        Ok(())
    }

    /// The provider says an account's sign-in stopped working: every enabled
    /// connection bound to it needs reconnecting. Returns (workspace, connection).
    pub(crate) async fn connector_account_expired(
        &self,
        provider: &str,
        account: &str,
        user: Option<&str>,
    ) -> Result<Vec<(String, String)>> {
        let found = rows(&self.db, "SELECT workspace,id FROM relay_control_connections WHERE provider=$1 AND enabled AND binding->>'account'=$2 AND ($3::text IS NULL OR binding->>'user'=$3) LIMIT 20",
            vec![provider.into(), account.into(), user.map(str::to_owned).into()]).await?;
        let mut marked = vec![];
        for row in found {
            let (w, c): (String, String) = (row.try_get("", "workspace")?, row.try_get("", "id")?);
            self.connector_needs_reconnect(&w, &c).await?;
            marked.push((w, c));
        }
        Ok(marked)
    }

    /// Subscriptions to stop at their provider, oldest attempt first.
    pub(crate) async fn subscriptions_to_release(
        &self,
        w: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Release>> {
        if !exists(&self.db).await? {
            return Ok(vec![]);
        }
        let found = rows(&self.db, "SELECT s.workspace,s.id,s.provider,s.provider_id,s.binding,c.server FROM relay_connector_subscriptions s JOIN relay_control_connections c ON c.workspace=s.workspace AND c.id=s.connection WHERE s.state<>'active' AND NOT s.released AND s.retry_ms<=$1 AND ($2::text IS NULL OR s.workspace=$2) ORDER BY s.retry_ms,s.id LIMIT $3",
            vec![(crate::identity::user::now() * 1000).into(), w.map(str::to_owned).into(), limit.into()]).await?;
        found
            .iter()
            .map(|r| {
                Ok(Release {
                    workspace: r.try_get("", "workspace")?,
                    id: r.try_get("", "id")?,
                    provider: r.try_get("", "provider")?,
                    provider_id: r.try_get("", "provider_id")?,
                    server: r.try_get("", "server")?,
                    binding: r.try_get("", "binding")?,
                })
            })
            .collect()
    }

    /// Records a clean-up attempt: done, or retried later with backoff (one
    /// minute doubling to an hour).
    pub(crate) async fn subscription_release_tried(
        &self,
        w: &str,
        id: &str,
        done: bool,
    ) -> Result<()> {
        execute(&self.db, "UPDATE relay_connector_subscriptions SET released=$3,attempts=attempts+1,retry_ms=CASE WHEN $3 THEN retry_ms ELSE (extract(epoch FROM clock_timestamp())*1000)::bigint+LEAST(3600000,60000*(2^LEAST(attempts,6))::bigint) END WHERE workspace=$1 AND id=$2 AND state<>'active'",
            vec![w.into(), id.into(), done.into()]).await?;
        Ok(())
    }
}
