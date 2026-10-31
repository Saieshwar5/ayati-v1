//! Connector provider records: sign-ins in progress, Cuckoo's switch per
//! provider, and accounts that need signing in again. Provider identities are
//! stored, never tokens.
use super::*;
use crate::identity::control::connections_model::WorkspaceConnection;
use serde_json::json;
#[path = "connector_providers_migration.rs"]
pub(super) mod migration;
#[cfg(test)]
#[path = "connector_providers_tests.rs"]
mod tests;

/// A sign-in waiting for the admin to finish at the provider.
pub(crate) struct SignIn {
    pub workspace: String,
    pub connection: String,
    pub server: String,
    pub provider: String,
    pub binding: Value,
    pub revision: i64,
}

/// What a sign-in records when it starts.
pub(crate) struct StartSignIn<'a> {
    pub id: &'a str,
    pub provider: &'a str,
    pub binding: &'a Value,
    pub expires: i64,
}

impl PgStore {
    /// Records a sign-in for `input`'s connection. The same checks as connecting
    /// directly apply: an admin, an approved app, and an unchanged connection.
    pub(crate) async fn connector_sign_in_start(
        &self,
        user: &User,
        w: &str,
        input: &WorkspaceConnection,
        start: &StartSignIn<'_>,
    ) -> Result<()> {
        crate::identity::worker_control::model::uuid(start.id)?;
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        super::workspace_connections::approved(&tx, w, &input.server).await?;
        execute(&tx, "INSERT INTO relay_connector_sign_ins(id,workspace,connection,server,provider,binding,revision,started_by,created,expires) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            vec![start.id.into(), w.into(), input.id.clone().into(), input.server.clone().into(), start.provider.into(),
                start.binding.clone().into(), input.revision.into(), user.id.clone().into(), crate::identity::user::now().into(), start.expires.into()]).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            "connector_sign_in_started",
            &input.server,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The waiting sign-in `id`, only for the admin who started it and only
    /// before it expires.
    pub(crate) async fn connector_sign_in(&self, user: &User, id: &str) -> Result<SignIn> {
        crate::identity::worker_control::model::uuid(id)?;
        let row = query(&self.db, "SELECT workspace,connection,server,provider,binding,revision,expires FROM relay_connector_sign_ins WHERE id=$1 AND started_by=$2 AND state='waiting'",
            vec![id.into(), user.id.clone().into()]).await?.context("This sign-in is no longer open. Connect the account again.")?;
        ensure!(
            row.try_get::<i64>("", "expires")? > crate::identity::user::now(),
            "This sign-in expired. Connect the account again."
        );
        let sign_in = SignIn {
            workspace: row.try_get("", "workspace")?,
            connection: row.try_get("", "connection")?,
            server: row.try_get("", "server")?,
            provider: row.try_get("", "provider")?,
            binding: row.try_get("", "binding")?,
            revision: row.try_get("", "revision")?,
        };
        let tx = self.read_tx(&sign_in.workspace).await?;
        manager(&tx, &sign_in.workspace, &user.id).await?;
        tx.commit().await?;
        Ok(sign_in)
    }

    pub(crate) async fn connector_sign_in_done(&self, id: &str, connected: bool) -> Result<()> {
        execute(
            &self.db,
            "UPDATE relay_connector_sign_ins SET state=$2 WHERE id=$1 AND state='waiting'",
            vec![
                id.into(),
                if connected { "connected" } else { "failed" }.into(),
            ],
        )
        .await?;
        Ok(())
    }

    /// Cuckoo's note when `provider` is switched off, or `None` while it runs.
    pub(crate) async fn connector_provider_paused(&self, provider: &str) -> Result<Option<String>> {
        Ok(query(
            &self.db,
            "SELECT note FROM relay_connector_providers WHERE provider=$1 AND NOT enabled",
            vec![provider.into()],
        )
        .await?
        .map(|r| r.try_get::<String>("", "note"))
        .transpose()?)
    }

    pub(crate) async fn connector_providers(&self) -> Result<Value> {
        let rows = json_rows(&self.db, "SELECT jsonb_build_object('provider',provider,'enabled',enabled,'note',note,'changed',changed) AS data FROM relay_connector_providers ORDER BY provider", vec![]).await?;
        Ok(json!({"providers": rows}))
    }

    /// Switches every connector through `provider` off or back on. Pending
    /// approvals stay pending; nothing is sent while it is off.
    pub(crate) async fn connector_provider_set(
        &self,
        user: &User,
        provider: &str,
        enabled: bool,
        note: &str,
    ) -> Result<()> {
        ensure!(
            !provider.is_empty()
                && provider.len() <= 32
                && provider
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "Invalid connector provider"
        );
        let note = note.trim();
        ensure!(
            note.chars().count() <= 300 && !note.chars().any(char::is_control),
            "Keep the note to one line of 300 characters"
        );
        ensure!(
            enabled || !note.is_empty(),
            "Say why the provider is switched off; people see this note"
        );
        let tx = self.db.begin().await?;
        execute(&tx, "INSERT INTO relay_connector_providers(provider,enabled,note,changed_by,changed) VALUES($1,$2,$3,$4,$5) ON CONFLICT(provider) DO UPDATE SET enabled=excluded.enabled,note=excluded.note,changed_by=excluded.changed_by,changed=excluded.changed",
            vec![provider.into(), enabled.into(), note.into(), user.id.clone().into(), crate::identity::user::now().into()]).await?;
        audit(
            &tx,
            &user.id,
            None,
            if enabled {
                "connector_provider_resumed"
            } else {
                "connector_provider_paused"
            },
            provider,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The account behind `connection` needs signing in again: agents stop
    /// seeing its actions until an admin reconnects it.
    pub(crate) async fn connector_needs_reconnect(&self, w: &str, connection: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        execute(&tx, "INSERT INTO relay_connector_reconnects(workspace,connection,since) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
            vec![w.into(), connection.into(), crate::identity::user::now().into()]).await?;
        tx.commit().await?;
        Ok(())
    }
}
