//! What built apps keep (Apps BA2): records in the collections their manifest declares,
//! read and written through the bridge, and the roles people hold in each app. Every
//! request is checked here against the manifest of the version being run and the
//! person's roles; the app's own code decides nothing.
use super::*;
use crate::identity::built_apps::manifest::{MAX_RECORDS, Manifest};
use crate::identity::library::model::Conflict;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;
use serde_json::json;

/// Schema 79: built apps' records and roles.
pub(crate) struct BuiltAppData;
impl MigrationName for BuiltAppData {
    fn name(&self) -> &str {
        "m20261105_000079_built_app_data"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for BuiltAppData {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("built_app_data.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Built app data requires a forward migration".into(),
        ))
    }
}

/// One bridge request from a running app.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DataRequest {
    #[serde(default)]
    pub preview: bool,
    pub op: String,
    #[serde(default)]
    pub collection: String,
    pub record: Option<String>,
    pub data: Option<Value>,
    pub revision: Option<i64>,
    #[serde(default)]
    pub mine: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RoleChange {
    pub role: String,
    pub people: Vec<String>,
}

const RECORD: &str = "jsonb_build_object('id',r.id,'data',r.data,'revision',r.revision,'created',r.created,'updated',r.updated,'mine',r.created_by=$5,
    'by',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=r.created_by),
    'agent',(SELECT g.name FROM relay_shared_agents g WHERE g.workspace=r.workspace AND g.id=r.by_agent)) AS data";

/// The manifest of a version; the published one passed its checks.
async fn manifest(tx: &DatabaseTransaction, w: &str, id: &str, version: i32) -> Result<Manifest> {
    let text: String = query(tx, "SELECT content FROM relay_built_app_files WHERE workspace=$1 AND app=$2 AND version=$3 AND path='cuckoo-app.json'", vec![w.into(), id.into(), version.into()]).await?.context("cuckoo-app.json is missing")?.try_get("", "content")?;
    Manifest::parse(&text)
}
async fn assigned(tx: &DatabaseTransaction, w: &str, id: &str, user: &str) -> Result<Vec<String>> {
    rows(tx, "SELECT role FROM relay_built_app_roles WHERE workspace=$1 AND app=$2 AND user_id=$3 ORDER BY role", vec![w.into(), id.into(), user.into()]).await?.into_iter().map(|r| Ok(r.try_get("", "role")?)).collect()
}

/// One data request for `user`, by the published (or preview) manifest and their roles.
/// `agent` is the agent acting for them, recorded on what it creates or changes.
pub(super) async fn answer(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    id: &str,
    input: &DataRequest,
    agent: Option<&str>,
) -> Result<Value> {
    let (found, _) = super::app(tx, user, w, id).await?;
    let (stage, version) = if input.preview {
        ensure!(
            found["manages"] == true,
            "Only its builder and admins preview a draft"
        );
        ("preview", 0)
    } else {
        ensure!(found["opens"] == true, "This app isn't published for you");
        ("live", found["published"].as_i64().unwrap_or(0) as i32)
    };
    let manifest = manifest(tx, w, id, version).await?;
    let roles: Vec<String> = if input.preview {
        manifest.roles.clone()
    } else {
        let mut held = vec![manifest.default_role().to_owned()];
        for role in assigned(tx, w, id, &user.id).await? {
            if manifest.roles.contains(&role) && !held.contains(&role) {
                held.push(role);
            }
        }
        held
    };
    if input.op == "me" {
        let name: String = query(
            tx,
            "SELECT COALESCE(display_name,email) AS name FROM relay_users WHERE id=$1",
            vec![user.id.clone().into()],
        )
        .await?
        .context("Person")?
        .try_get("", "name")?;
        return Ok(json!({"id":user.id,"name":name,"roles":roles}));
    }
    let action = match input.op.as_str() {
        "list" | "get" => "read",
        "create" | "update" | "delete" => input.op.as_str(),
        _ => anyhow::bail!("Ask for me, list, get, create, update or delete"),
    };
    let collection = input.collection.as_str();
    ensure!(
        manifest.collections.contains_key(collection),
        "The app has no collection \"{collection}\""
    );
    let any = manifest
        .allows(&roles, collection, action)
        .with_context(|| format!("Your role in this app can't {action} {collection}"))?;
    let values = |extra: Vec<sea_orm::Value>| -> Vec<sea_orm::Value> {
        let mut v: Vec<sea_orm::Value> = vec![
            w.into(),
            id.into(),
            stage.into(),
            collection.into(),
            user.id.clone().into(),
        ];
        v.extend(extra);
        v
    };
    // $1 workspace, $2 app, $3 stage, $4 collection, $5 person.
    let scope = "r.workspace=$1 AND r.app=$2 AND r.stage=$3 AND r.collection=$4";
    let result = match input.op.as_str() {
        "list" => {
            let own = !any || input.mine;
            let records = json_rows(tx, &format!("SELECT {RECORD} FROM relay_built_app_records r WHERE {scope} AND (NOT $6 OR r.created_by=$5) ORDER BY r.created DESC,r.id LIMIT 200"), values(vec![own.into()])).await?;
            json!({"records":records})
        }
        "get" => {
            let record = input.record.clone().context("Say which record")?;
            let found = json_rows(tx, &format!("SELECT {RECORD} FROM relay_built_app_records r WHERE {scope} AND r.id=$6 AND ($7 OR r.created_by=$5)"), values(vec![record.into(), any.into()])).await?;
            found
                .into_iter()
                .next()
                .context("No such record, or not one you can read")?
        }
        "create" => {
            let data = input.data.clone().context("Send the record's fields")?;
            manifest.record(collection, &data)?;
            let count: i64 = query(tx, "SELECT count(*) AS n FROM relay_built_app_records r WHERE r.workspace=$1 AND r.app=$2 AND r.stage=$3", vec![w.into(), id.into(), stage.into()]).await?.context("Count")?.try_get("", "n")?;
            ensure!(
                count < MAX_RECORDS,
                "An app keeps up to {MAX_RECORDS} records"
            );
            let record = uuid::Uuid::new_v4().to_string();
            execute(tx, "INSERT INTO relay_built_app_records(workspace,app,stage,collection,id,data,created_by,updated_by,created,updated,by_agent) VALUES($1,$2,$3,$4,$6,$7,$5,$5,$8,$8,$9)", values(vec![record.clone().into(), data.into(), now().into(), agent.map(str::to_owned).into()])).await?;
            if stage == "live" {
                let added = super::events::Happened {
                    app: id,
                    collection,
                    record: &record,
                    kind: "added",
                    revision: 1,
                    changes: None,
                };
                super::events::queue(tx, w, added, agent).await?;
            }
            json_rows(
                tx,
                &format!(
                    "SELECT {RECORD} FROM relay_built_app_records r WHERE {scope} AND r.id=$6"
                ),
                values(vec![record.into()]),
            )
            .await?
            .into_iter()
            .next()
            .context("Record")?
        }
        _ => {
            let record = input.record.clone().context("Say which record")?;
            let row = query(tx, &format!("SELECT r.data,r.revision,r.created_by FROM relay_built_app_records r WHERE {scope} AND r.id=$6 FOR UPDATE"), values(vec![record.clone().into()])).await?.context("No such record")?;
            let owner: String = row.try_get("", "created_by")?;
            ensure!(
                any || owner == user.id,
                "Your role in this app can only {action} your own {collection}"
            );
            if input.op == "delete" {
                execute(
                    tx,
                    &format!("DELETE FROM relay_built_app_records r WHERE {scope} AND r.id=$6"),
                    values(vec![record.clone().into()]),
                )
                .await?;
                json!({"id":record,"deleted":true})
            } else {
                let revision: i64 = row.try_get("", "revision")?;
                if input.revision != Some(revision) {
                    return Err(Conflict.into());
                }
                let mut data: Value = row.try_get("", "data")?;
                let changes = input.data.clone().context("Send the fields to change")?;
                let changes = changes
                    .as_object()
                    .context("A record is an object of fields")?;
                for (field, value) in changes {
                    data[field] = value.clone();
                }
                manifest.record(collection, &data)?;
                execute(tx, &format!("UPDATE relay_built_app_records r SET data=$7,revision=revision+1,updated_by=$5,updated=$8,by_agent=$9 WHERE {scope} AND r.id=$6"), values(vec![record.clone().into(), data.into(), now().into(), agent.map(str::to_owned).into()])).await?;
                if stage == "live" {
                    let changed = super::events::Happened {
                        app: id,
                        collection,
                        record: &record,
                        kind: "changed",
                        revision: revision + 1,
                        changes: Some(Value::Object(changes.clone())),
                    };
                    super::events::queue(tx, w, changed, agent).await?;
                }
                json_rows(
                    tx,
                    &format!(
                        "SELECT {RECORD} FROM relay_built_app_records r WHERE {scope} AND r.id=$6"
                    ),
                    values(vec![record.into()]),
                )
                .await?
                .into_iter()
                .next()
                .context("Record")?
            }
        }
    };
    Ok(result)
}

impl PgStore {
    /// Answers one bridge request for the person running the app: the published version
    /// with live records, or (builder and admins) the draft with preview records and
    /// every role, so they can try each screen.
    pub(crate) async fn built_app_data(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &DataRequest,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let result = answer(&tx, user, w, id, input, None).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// The app's roles and who holds each, for its builder and admins.
    pub(crate) async fn built_app_roles(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let (found, _) = super::app(&tx, user, w, id).await?;
        ensure!(
            found["manages"] == true,
            "Its builder or an admin gives roles in an app"
        );
        let version = found["published"].as_i64().unwrap_or(0) as i32;
        let roles = manifest(&tx, w, id, version)
            .await
            .map(|m| m.roles)
            .unwrap_or_default();
        let held = json_rows(&tx, "SELECT jsonb_build_object('role',r.role,'id',u.id,'name',COALESCE(u.display_name,u.email)) AS data FROM relay_built_app_roles r JOIN relay_users u ON u.id=r.user_id WHERE r.workspace=$1 AND r.app=$2 ORDER BY r.role,u.id", vec![w.into(), id.into()]).await?;
        tx.commit().await?;
        Ok(json!({"roles":roles,"held":held,"from_version":version}))
    }

    /// Its builder or an admin chooses who holds one of the app's roles. The first role
    /// is everyone's, so it isn't given.
    pub(crate) async fn built_app_roles_set(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &RoleChange,
    ) -> Result<Value> {
        ensure!(input.people.len() <= 500, "Choose up to 500 people");
        let tx = self.workspace_tx(w).await?;
        let (found, _) = super::app(&tx, user, w, id).await?;
        ensure!(
            found["manages"] == true,
            "Its builder or an admin gives roles in an app"
        );
        let version = found["published"].as_i64().unwrap_or(0) as i32;
        let manifest = manifest(&tx, w, id, version).await?;
        ensure!(
            manifest.roles.contains(&input.role),
            "The app has no role \"{}\"",
            input.role
        );
        ensure!(
            manifest.default_role() != input.role,
            "Everyone who can open the app holds \"{}\"",
            input.role
        );
        execute(
            &tx,
            "DELETE FROM relay_built_app_roles WHERE workspace=$1 AND app=$2 AND role=$3",
            vec![w.into(), id.into(), input.role.clone().into()],
        )
        .await?;
        for person in &input.people {
            role(&tx, w, person)
                .await
                .context("Choose people in this workspace")?;
            execute(&tx, "INSERT INTO relay_built_app_roles(workspace,app,user_id,role) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING", vec![w.into(), id.into(), person.clone().into(), input.role.clone().into()]).await?;
        }
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "built_app_roles",
            id,
            &format!(
                "Gave the role \"{}\" in \"{}\" to {} {}",
                input.role,
                found["name"].as_str().unwrap_or(""),
                input.people.len(),
                if input.people.len() == 1 {
                    "person"
                } else {
                    "people"
                }
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"ok":true}))
    }
}

#[cfg(test)]
#[path = "built_app_data_tests.rs"]
mod tests;
