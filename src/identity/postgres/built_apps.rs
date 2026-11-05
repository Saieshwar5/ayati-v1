//! Built apps in PostgreSQL (Apps AP4–AP9, first part): records, people, and each
//! version's files. Anyone can start an app from the starter; its builder (the person)
//! changes the draft, runs the checks and publishes; admins can roll back or remove any
//! app. People open a published app only if it is for everyone or they were chosen.
use super::*;
use crate::identity::built_apps::files as built_apps;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;
use serde_json::json;

/// Schema 78: built apps and their versions' files.
pub(crate) struct BuiltApps;
impl MigrationName for BuiltApps {
    fn name(&self) -> &str {
        "m20261105_000078_built_apps"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for BuiltApps {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("built_apps.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Built apps require a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewApp {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub audience: String,
    #[serde(default)]
    pub people: Vec<String>,
}

/// May open the published app: everyone, the people chosen, its builder, or an admin.
const OPENS: &str = "(a.published IS NOT NULL AND (a.audience='everyone' OR a.builder=$2 OR $3 OR EXISTS(SELECT 1 FROM relay_built_app_people p WHERE p.workspace=a.workspace AND p.app=a.id AND p.user_id=$2)))";

async fn app(tx: &DatabaseTransaction, user: &User, w: &str, id: &str) -> Result<(Value, bool)> {
    let admin = role(tx, w, &user.id).await? == "Admin";
    let found = json_rows(tx, &format!("SELECT to_jsonb(a) || jsonb_build_object('opens',{OPENS},'manages',a.builder=$2 OR $3) AS data FROM relay_built_apps a WHERE a.workspace=$1 AND a.id=$4 AND NOT a.removed"), vec![w.into(), user.id.clone().into(), admin.into(), id.into()]).await?.into_iter().next();
    let found = found
        .filter(|a| a["opens"] == true || a["manages"] == true)
        .context("This app is unavailable")?;
    Ok((found, admin))
}

impl PgStore {
    /// Built apps as the viewer sees them; drafts only for their builder and admins.
    pub(super) async fn built_apps_view(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        w: &str,
        admin: bool,
    ) -> Result<Vec<Value>> {
        let present: bool = query(
            tx,
            "SELECT to_regclass('relay_built_apps') IS NOT NULL AS present",
            vec![],
        )
        .await?
        .context("Schema check")?
        .try_get("", "present")?;
        if !present {
            return Ok(vec![]);
        }
        json_rows(tx, &format!("SELECT jsonb_build_object('id',a.id,'kind','built','name',a.name,'description',a.description,'audience',a.audience,'version',a.published,'draft_revision',a.draft_revision,
            'builder_name',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=a.builder),'is_builder',a.builder=$2,'can_open',{OPENS},'can_manage',a.builder=$2 OR $3,
            'people',CASE WHEN a.builder=$2 OR $3 THEN (SELECT COALESCE(jsonb_agg(jsonb_build_object('id',u.id,'name',COALESCE(u.display_name,u.email))),'[]'::jsonb) FROM relay_built_app_people p JOIN relay_users u ON u.id=p.user_id WHERE p.workspace=a.workspace AND p.app=a.id) END,
            'versions',CASE WHEN a.builder=$2 OR $3 THEN (SELECT COALESCE(jsonb_agg(jsonb_build_object('version',v.version,'note',v.note,'at',v.published_at) ORDER BY v.version DESC),'[]'::jsonb) FROM relay_built_app_versions v WHERE v.workspace=a.workspace AND v.app=a.id) END) AS data
            FROM relay_built_apps a WHERE a.workspace=$1 AND NOT a.removed AND (a.published IS NOT NULL OR a.builder=$2 OR $3) ORDER BY a.name,a.id"), vec![w.into(), user.id.clone().into(), admin.into()]).await
    }

    pub(crate) async fn built_app_create(
        &self,
        user: &User,
        w: &str,
        input: &NewApp,
    ) -> Result<Value> {
        crate::identity::library::model::uuid(&input.id)?;
        let name = input.name.trim();
        ensure!(
            !name.is_empty() && name.chars().count() <= 80,
            "Name the app in up to 80 characters"
        );
        ensure!(
            input.description.chars().count() <= 300,
            "Describe it in up to 300 characters"
        );
        ensure!(
            ["everyone", "selected"].contains(&input.audience.as_str()),
            "Choose who can use it"
        );
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        if query(
            &tx,
            "SELECT 1 AS found FROM relay_built_apps WHERE workspace=$1 AND id=$2",
            vec![w.into(), input.id.clone().into()],
        )
        .await?
        .is_some()
        {
            tx.commit().await?;
            return Ok(json!({"id":input.id}));
        }
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_built_apps WHERE workspace=$1 AND NOT removed",
            vec![w.into()],
        )
        .await?
        .context("Count")?
        .try_get("", "n")?;
        ensure!(count < 100, "A workspace can build up to 100 apps");
        execute(&tx, "INSERT INTO relay_built_apps(workspace,id,name,description,builder,audience,created,updated) VALUES($1,$2,$3,$4,$5,$6,$7,$7)", vec![w.into(), input.id.clone().into(), name.into(), input.description.trim().into(), user.id.clone().into(), input.audience.clone().into(), now().into()]).await?;
        for person in &input.people {
            role(&tx, w, person)
                .await
                .context("Choose people in this workspace")?;
            execute(&tx, "INSERT INTO relay_built_app_people(workspace,app,user_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", vec![w.into(), input.id.clone().into(), person.clone().into()]).await?;
        }
        for (path, content) in built_apps::starter(name, input.description.trim()) {
            execute(&tx, "INSERT INTO relay_built_app_files(workspace,app,version,path,content) VALUES($1,$2,0,$3,$4)", vec![w.into(), input.id.clone().into(), path.into(), content.into()]).await?;
        }
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "built_app_created",
            &input.id,
            &format!("Started building the app \"{name}\""),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"id":input.id}))
    }

    /// Its builder or an admin renames the app or changes who may use it.
    pub(crate) async fn built_app_settings(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &NewApp,
    ) -> Result<Value> {
        ensure!(input.id == id, "Change one app at a time");
        let name = input.name.trim();
        ensure!(
            !name.is_empty() && name.chars().count() <= 80,
            "Name the app in up to 80 characters"
        );
        ensure!(
            input.description.chars().count() <= 300,
            "Describe it in up to 300 characters"
        );
        ensure!(
            ["everyone", "selected"].contains(&input.audience.as_str()),
            "Choose who can use it"
        );
        let tx = self.workspace_tx(w).await?;
        let (found, _) = app(&tx, user, w, id).await?;
        ensure!(
            found["manages"] == true,
            "Its builder or an admin changes an app"
        );
        execute(&tx, "UPDATE relay_built_apps SET name=$3,description=$4,audience=$5,updated=$6 WHERE workspace=$1 AND id=$2", vec![w.into(), id.into(), name.into(), input.description.trim().into(), input.audience.clone().into(), now().into()]).await?;
        execute(
            &tx,
            "DELETE FROM relay_built_app_people WHERE workspace=$1 AND app=$2",
            vec![w.into(), id.into()],
        )
        .await?;
        for person in &input.people {
            role(&tx, w, person)
                .await
                .context("Choose people in this workspace")?;
            execute(&tx, "INSERT INTO relay_built_app_people(workspace,app,user_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", vec![w.into(), id.into(), person.clone().into()]).await?;
        }
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "built_app_settings",
            id,
            &format!(
                "Changed the app \"{name}\" for {}",
                if input.audience == "everyone" {
                    "everyone".to_owned()
                } else {
                    format!("{} chosen people", input.people.len())
                }
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"ok":true}))
    }

    /// Its builder or an admin removes the app; its versions stay for the audit trail.
    pub(crate) async fn built_app_remove(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let (found, _) = app(&tx, user, w, id).await?;
        ensure!(
            found["manages"] == true,
            "Its builder or an admin removes an app"
        );
        execute(&tx, "UPDATE relay_built_apps SET removed=true,published=NULL,updated=$3 WHERE workspace=$1 AND id=$2", vec![w.into(), id.into(), now().into()]).await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "built_app_removed",
            id,
            &format!(
                "Removed the app \"{}\"",
                found["name"].as_str().unwrap_or("")
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"ok":true}))
    }
}

#[path = "built_app_code.rs"]
mod code;
pub(crate) use code::{AppFile, AppRelease};
#[path = "built_app_data.rs"]
mod data;
pub(crate) use data::{BuiltAppData, DataRequest, RoleChange};

#[cfg(test)]
#[path = "built_apps_tests.rs"]
mod tests;
