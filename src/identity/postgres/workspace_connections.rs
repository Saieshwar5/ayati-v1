//! Workspace approval and connection lifecycle. No credentials are stored in responses.
use super::*;
use crate::identity::control::connections_model::{SystemApproval, WorkspaceConnection};
use serde_json::json;

pub(super) fn stale_revision(actual: i64, expected: i64, same: bool) -> Result<bool> {
    ensure!((0..i64::MAX).contains(&expected), "Invalid revision");
    if actual == expected + 1 && same {
        return Ok(true);
    }
    ensure!(actual == expected, "Settings changed; reload before saving");
    Ok(false)
}

pub(super) async fn approved(db: &impl ConnectionTrait, w: &str, server: &str) -> Result<()> {
    ensure!(query(db,"SELECT server FROM relay_control_systems WHERE workspace=$1 AND server=$2 AND approved",vec![w.into(),server.into()]).await?.is_some(), "A workspace admin must approve this system first");
    Ok(())
}

/// A re-reviewed, reconnected or disconnected account starts over: its actions may
/// mean something new, so every rule for it and every agent's switch is removed.
/// Old permissions never authorize changed credentials or operation meanings.
pub(super) async fn reset_connection(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<()> {
    execute(
        db,
        "DELETE FROM relay_agent_connections WHERE workspace=$1 AND connection=$2",
        vec![w.into(), id.into()],
    )
    .await?;
    execute(
        db,
        "DELETE FROM relay_control_rules WHERE workspace=$1 AND connection=$2",
        vec![w.into(), id.into()],
    )
    .await?;
    execute(
        db,
        "DELETE FROM relay_connector_rules WHERE workspace=$1 AND connection=$2",
        vec![w.into(), id.into()],
    )
    .await?;
    super::control_rules::invalidate(db, w, id, None, None).await
}

impl PgStore {
    pub(crate) async fn workspace_connections(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let systems=json_rows(&tx,"SELECT to_jsonb(s) AS data FROM relay_control_systems s WHERE workspace=$1 ORDER BY server",vec![w.into()]).await?;
        let connections=json_rows(&tx,"SELECT jsonb_build_object('id',id,'server',server,'definition',definition,'enabled',enabled,'revision',revision) AS data FROM relay_control_connections WHERE workspace=$1 ORDER BY server",vec![w.into()]).await?;
        let people=json_rows(&tx,"SELECT jsonb_build_object('id',u.id,'name',COALESCE(u.display_name,u.email),'role',m.role) AS data FROM relay_members m JOIN relay_users u ON u.id=m.user_id WHERE m.workspace=$1 ORDER BY m.sequence",vec![w.into()]).await?;
        let safety = json_rows(&tx, include_str!("workspace_safety.sql"), vec![w.into()]).await?;
        tx.commit().await?;
        Ok(
            json!({"systems":systems,"connections":connections,"safety":safety,"people":people,"viewer":user.id}),
        )
    }

    pub(crate) async fn control_system(
        &self,
        user: &User,
        w: &str,
        input: &SystemApproval,
    ) -> Result<()> {
        crate::identity::identifier(&input.server)?;
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let old = query(
            &tx,
            "SELECT approved,revision FROM relay_control_systems WHERE workspace=$1 AND server=$2",
            vec![w.into(), input.server.clone().into()],
        )
        .await?;
        let actual = old
            .as_ref()
            .map(|r| r.try_get::<i64>("", "revision"))
            .transpose()?
            .unwrap_or(0);
        let same = old
            .as_ref()
            .map(|r| r.try_get::<bool>("", "approved"))
            .transpose()?
            == Some(input.approved);
        if stale_revision(actual, input.revision, same)? {
            tx.commit().await?;
            return Ok(());
        }
        execute(&tx,"INSERT INTO relay_control_systems(workspace,server,approved,revision) VALUES($1,$2,$3,1) ON CONFLICT(workspace,server) DO UPDATE SET approved=excluded.approved,revision=relay_control_systems.revision+1",vec![w.into(),input.server.clone().into(),input.approved.into()]).await?;
        if !input.approved && let Some(row)=query(&tx,"UPDATE relay_control_connections SET enabled=false,revision=revision+1 WHERE workspace=$1 AND server=$2 RETURNING id",vec![w.into(),input.server.clone().into()]).await? {
            reset_connection(&tx,w,&row.try_get::<String>("","id")?).await?;
        }
        audit(
            &tx,
            &user.id,
            Some(w),
            "control_system_approval_saved",
            &input.server,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn workspace_connection_save(
        &self,
        user: &User,
        w: &str,
        input: &WorkspaceConnection,
        definition: Option<(Value, &str)>,
    ) -> Result<()> {
        crate::identity::worker_control::model::uuid(&input.id)?;
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        if input.enabled {
            approved(&tx, w, &input.server).await?;
        }
        let old=query(&tx,"SELECT id,server,fingerprint,enabled,revision FROM relay_control_connections WHERE workspace=$1 AND (id=$2 OR server=$3)",vec![w.into(),input.id.clone().into(),input.server.clone().into()]).await?;
        if let Some(row) = &old {
            ensure!(
                row.try_get::<String>("", "id")? == input.id
                    && row.try_get::<String>("", "server")? == input.server,
                "Connection already exists; reload before connecting"
            );
        }
        let actual = old
            .as_ref()
            .map(|r| r.try_get::<i64>("", "revision"))
            .transpose()?
            .unwrap_or(0);
        let same = if let Some(row) = &old {
            row.try_get::<bool>("", "enabled")? == input.enabled
                && (!input.enabled
                    || definition.as_ref().is_some_and(|(_, fp)| {
                        row.try_get::<String>("", "fingerprint").ok().as_deref() == Some(*fp)
                    }))
        } else {
            false
        };
        if stale_revision(actual, input.revision, same)? {
            tx.commit().await?;
            return Ok(());
        }
        if input.enabled {
            let (value, fingerprint) = definition.context("Verified connection required")?;
            ensure!(value["id"] == input.server, "Connection identity changed");
            execute(&tx,"INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint) VALUES($1,$2,$3,$4,$5) ON CONFLICT(workspace,id) DO UPDATE SET definition=excluded.definition,fingerprint=excluded.fingerprint,enabled=true,revision=relay_control_connections.revision+1",vec![w.into(),input.id.clone().into(),input.server.clone().into(),value.into(),fingerprint.into()]).await?;
        } else {
            ensure!(old.is_some(), "Connection unavailable");
            execute(&tx,"UPDATE relay_control_connections SET enabled=false,revision=revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),input.id.clone().into()]).await?;
        }
        if old.is_some() {
            reset_connection(&tx, w, &input.id).await?;
        }
        audit(
            &tx,
            &user.id,
            Some(w),
            "workspace_connection_saved",
            &input.id,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn connection_preflight(
        &self,
        user: &User,
        w: &str,
        input: &WorkspaceConnection,
    ) -> Result<()> {
        let tx = self.read_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        approved(&tx, w, &input.server).await?;
        let row=query(&tx,"SELECT id,revision FROM relay_control_connections WHERE workspace=$1 AND (id=$2 OR server=$3)",vec![w.into(),input.id.clone().into(),input.server.clone().into()]).await?;
        if let Some(row) = row {
            ensure!(
                row.try_get::<String>("", "id")? == input.id,
                "Connection already exists; reload before connecting"
            );
            let rev = row.try_get::<i64>("", "revision")?;
            ensure!(
                input.revision >= 0
                    && input.revision < i64::MAX
                    && (rev == input.revision || rev == input.revision + 1),
                "Connection changed; reload before connecting"
            );
        } else {
            ensure!(input.revision == 0, "Connection unavailable");
        }
        tx.commit().await?;
        Ok(())
    }
}
