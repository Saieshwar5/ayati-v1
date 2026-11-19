//! Schema 135: apps a workspace's admins added from a provider's library.
//! The row holds the admin's review ([`AddedApp`]); allowing, asking and
//! connecting use the same tables as built-in apps. Adding an app allows it.
//! Changing its actions or removing it ends every connection of it, because a
//! saved connection carries the review it was made under.
use super::super::workspace_connections::{Released, released, reset_connection, stale_revision};
use super::super::*;
use super::{decided, settle_requests};
use crate::identity::connectors::added::{AddedAction, AddedApp, MAX_APPS};

#[cfg(test)]
#[path = "added_tests.rs"]
mod tests;

pub(crate) use migration::AddedApps;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct AddedApps;
    impl MigrationName for AddedApps {
        fn name(&self) -> &str {
            "m20261119_000135_added_apps"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for AddedApps {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("added.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Added apps require a forward migration".into(),
            ))
        }
    }
}

/// Ends every connection of `app`: its rules and switches are removed and each
/// account is returned for its provider to revoke.
async fn end_connections(db: &impl ConnectionTrait, w: &str, app: &str) -> Result<Vec<Released>> {
    let ended = rows(db,
        "UPDATE relay_control_connections SET enabled=false,revision=revision+1 WHERE workspace=$1 AND server=$2 AND enabled RETURNING id,server,provider,binding",
        vec![w.into(), app.into()]).await?;
    let mut accounts = vec![];
    for row in &ended {
        reset_connection(db, w, &row.try_get::<String>("", "id")?).await?;
        accounts.extend(released(row)?);
    }
    Ok(accounts)
}

fn read(row: &sea_orm::QueryResult) -> Result<AddedApp> {
    let actions: Value = row.try_get("", "actions")?;
    Ok(AddedApp {
        id: row.try_get("", "app")?,
        name: row.try_get("", "name")?,
        description: row.try_get("", "description")?,
        category: row.try_get("", "category")?,
        provider: row.try_get("", "provider")?,
        toolkit: row.try_get("", "toolkit")?,
        actions: serde_json::from_value::<Vec<AddedAction>>(actions)?,
    })
}

impl PgStore {
    /// The apps this workspace's admins added. An app whose row no longer reads
    /// (or now clashes with a built-in app) is left out, not an error: the
    /// workspace's other apps keep working.
    pub(crate) async fn added_apps(&self, w: &str) -> Result<Vec<AddedApp>> {
        let found = rows(
            &self.db,
            "SELECT app,name,description,category,provider,toolkit,actions FROM relay_added_apps WHERE workspace=$1 ORDER BY app",
            vec![w.into()],
        )
        .await?;
        Ok(found
            .iter()
            .filter_map(|row| read(row).ok().filter(|app| app.validate().is_ok()))
            .collect())
    }

    /// What Settings shows admins about each added app: `{app: {revision,
    /// toolkit, actions, added: {at, by_name}}}`.
    pub(crate) async fn added_apps_facts(&self, w: &str) -> Result<Value> {
        let found = json_rows(&self.db,
            "SELECT jsonb_build_object('id',a.app,'revision',a.revision,'toolkit',a.toolkit,
               'actions',(SELECT COALESCE(jsonb_agg(jsonb_build_object('tool',x->>'tool','title',x->>'title','effect',x->>'effect')),'[]'::jsonb) FROM jsonb_array_elements(a.actions) x),
               'added',jsonb_build_object('at',a.added_at,'by_name',COALESCE(u.display_name,u.email))) AS data
             FROM relay_added_apps a LEFT JOIN relay_users u ON u.id=a.added_by WHERE a.workspace=$1",
            vec![w.into()]).await?;
        let mut facts = serde_json::Map::new();
        for mut fact in found {
            if let Some(id) = fact["id"].as_str().map(str::to_owned) {
                fact.as_object_mut().map(|f| f.remove("id"));
                facts.insert(id, fact);
            }
        }
        Ok(Value::Object(facts))
    }

    /// An admin adds an app from the library, or changes the actions of one
    /// they added (`revision` 0 adds). Adding allows the app for the workspace.
    /// Changing it ends every connection of it; the accounts are returned for
    /// the provider to revoke.
    pub(crate) async fn added_app_save(
        &self,
        user: &User,
        w: &str,
        app: &AddedApp,
        revision: i64,
    ) -> Result<Vec<Released>> {
        app.validate()?;
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins choose the workspace's apps"
        );
        let old = query(
            &tx,
            "SELECT revision,toolkit,provider,actions FROM relay_added_apps WHERE workspace=$1 AND app=$2",
            vec![w.into(), app.id.clone().into()],
        )
        .await?;
        let actions = serde_json::to_value(&app.actions)?;
        let (actual, same) = match &old {
            Some(row) => (
                row.try_get::<i64>("", "revision")?,
                row.try_get::<Value>("", "actions")? == actions
                    && row.try_get::<String>("", "toolkit")? == app.toolkit,
            ),
            None => (0, false),
        };
        if stale_revision(actual, revision, same)? {
            tx.commit().await?;
            return Ok(vec![]);
        }
        let mut accounts = vec![];
        match &old {
            Some(row) => {
                ensure!(
                    row.try_get::<String>("", "toolkit")? == app.toolkit
                        && row.try_get::<String>("", "provider")? == app.provider,
                    "This is another app; remove the old one first"
                );
                accounts = end_connections(&tx, w, &app.id).await?;
                execute(&tx, "UPDATE relay_added_apps SET name=$3,description=$4,category=$5,actions=$6,revision=revision+1,changed_at=$7 WHERE workspace=$1 AND app=$2",
                    vec![w.into(), app.id.clone().into(), app.name.clone().into(), app.description.clone().into(), app.category.clone().into(), actions.into(), now().into()]).await?;
            }
            None => {
                let count = query(
                    &tx,
                    "SELECT count(*) AS n FROM relay_added_apps WHERE workspace=$1",
                    vec![w.into()],
                )
                .await?
                .map_or(Ok(0), |r| r.try_get::<i64>("", "n"))?;
                ensure!(
                    (count as usize) < MAX_APPS,
                    "A workspace adds at most {MAX_APPS} apps; remove one first"
                );
                // The name must be free: an operator's own system may hold it.
                ensure!(
                    query(&tx, "SELECT 1 AS used FROM relay_control_connections WHERE workspace=$1 AND server=$2 AND enabled LIMIT 1", vec![w.into(), app.id.clone().into()]).await?.is_none(),
                    "Another connection already uses this app's name"
                );
                execute(&tx, "INSERT INTO relay_added_apps(workspace,app,name,description,category,provider,toolkit,actions,added_by,added_at,changed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)",
                    vec![w.into(), app.id.clone().into(), app.name.clone().into(), app.description.clone().into(), app.category.clone().into(), app.provider.clone().into(), app.toolkit.clone().into(), actions.into(), user.id.clone().into(), now().into()]).await?;
            }
        }
        // Adding an app is the admin's yes to it.
        execute(&tx, "INSERT INTO relay_control_systems(workspace,server,approved,revision) VALUES($1,$2,true,1) ON CONFLICT(workspace,server) DO UPDATE SET approved=true,revision=relay_control_systems.revision+1",
            vec![w.into(), app.id.clone().into()]).await?;
        decided(&tx, w, &app.id, &user.id, "").await?;
        settle_requests(&tx, w, &app.id, true, &user.id, "").await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            if old.is_some() {
                "added_app_changed"
            } else {
                "added_app_saved"
            },
            &app.id,
            &format!(
                "{} {} with {} action{}",
                if old.is_some() {
                    "Changed the actions of"
                } else {
                    "Added the app"
                },
                app.name,
                app.actions.len(),
                if app.actions.len() == 1 { "" } else { "s" }
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(accounts)
    }

    /// An admin removes an app they added: every connection of it ends and it
    /// is no longer offered. Removing one that is already gone succeeds.
    pub(crate) async fn added_app_remove(
        &self,
        user: &User,
        w: &str,
        app: &str,
        revision: i64,
    ) -> Result<Vec<Released>> {
        crate::identity::identifier(app)?;
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins choose the workspace's apps"
        );
        let Some(row) = query(
            &tx,
            "SELECT revision,name FROM relay_added_apps WHERE workspace=$1 AND app=$2",
            vec![w.into(), app.into()],
        )
        .await?
        else {
            tx.commit().await?;
            return Ok(vec![]);
        };
        ensure!(
            row.try_get::<i64>("", "revision")? == revision,
            "Settings changed; reload before saving"
        );
        let accounts = end_connections(&tx, w, app).await?;
        execute(
            &tx,
            "DELETE FROM relay_added_apps WHERE workspace=$1 AND app=$2",
            vec![w.into(), app.into()],
        )
        .await?;
        execute(
            &tx,
            "DELETE FROM relay_control_systems WHERE workspace=$1 AND server=$2",
            vec![w.into(), app.into()],
        )
        .await?;
        execute(&tx, "UPDATE relay_app_requests SET state='declined',decided_by=$3,decided=$4,note='The app was removed' WHERE workspace=$1 AND connector=$2 AND state='pending'",
            vec![w.into(), app.into(), user.id.clone().into(), now().into()]).await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "added_app_removed",
            app,
            &format!("Removed the app {}", row.try_get::<String>("", "name")?),
        )
        .await?;
        tx.commit().await?;
        Ok(accounts)
    }
}
