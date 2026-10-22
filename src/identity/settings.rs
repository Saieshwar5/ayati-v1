//! Owner-only workspace configuration; personal preferences use existing profile APIs.
use super::*;
use axum::{Json, extract::Path as RoutePath, http::HeaderMap};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Update {
    name: String,
    previous_name: String,
}
pub(super) async fn read(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(match &app.store {
        database::Database::Sqlite(db) => db.workspace_settings(&user, &w)?,
        database::Database::Postgres(db) => db.workspace_settings(&user, &w).await?,
    }))
}
pub(super) async fn update(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
    Json(input): Json<Update>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let name = input.name.trim();
    check(
        !name.is_empty() && name.len() <= 80 && !name.chars().any(char::is_control),
        "Use a workspace name between 1 and 80 characters",
    )?;
    match &app.store {
        database::Database::Sqlite(db) => {
            db.rename_workspace(&user, &w, name, &input.previous_name)?
        }
        database::Database::Postgres(db) => {
            db.rename_workspace(&user, &w, name, &input.previous_name)
                .await?
        }
    }
    Ok(Json(json!({"name":name})))
}
pub(super) async fn activity(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(match &app.store {
        database::Database::Sqlite(db) => db.workspace_activity(&user, &w)?,
        database::Database::Postgres(db) => db.workspace_activity(&user, &w).await?,
    }))
}
impl Store {
    pub fn workspace_settings(&self, user: &User, w: &str) -> Result<Value> {
        let db = self.0.lock().unwrap();
        store::manager(&db, w, &user.id)?;
        let name: String =
            db.query_row("SELECT name FROM workspaces WHERE id=?1", [w], |r| r.get(0))?;
        Ok(json!({"name":name}))
    }
    pub fn rename_workspace(&self, user: &User, w: &str, name: &str, previous: &str) -> Result<()> {
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        store::manager(&tx, w, &user.id)?;
        anyhow::ensure!(
            tx.execute(
                "UPDATE workspaces SET name=?2 WHERE id=?1 AND name=?3",
                params![w, name, previous]
            )? == 1,
            "Workspace changed; reload before saving"
        );
        store::audit(&tx, &user.id, w, "workspace_renamed", name)?;
        tx.commit()?;
        Ok(())
    }
    pub fn workspace_activity(&self, user: &User, w: &str) -> Result<Value> {
        let db = self.0.lock().unwrap();
        store::manager(&db, w, &user.id)?;
        let mut q = db.prepare("SELECT a.id,COALESCE(u.display_name,u.email,a.actor),a.action,a.target,a.at FROM audit a LEFT JOIN users u ON u.id=a.actor WHERE a.workspace=?1 ORDER BY a.id DESC LIMIT 100")?;
        let items = q.query_map([w], |r| Ok(json!({"id":r.get::<_,i64>(0)?,"actor":r.get::<_,String>(1)?,"action":r.get::<_,String>(2)?,"target":r.get::<_,String>(3)?,"at":r.get::<_,i64>(4)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!({"activity":items}))
    }
}
