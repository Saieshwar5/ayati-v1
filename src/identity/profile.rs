//! User-chosen labels are independent of authentication and workspace authority.
use super::*;
use axum::{Json, http::HeaderMap};
use rusqlite::{Connection, params};
use serde::Deserialize;

pub(super) fn schema(db: &Connection) -> Result<()> {
    let columns = db
        .prepare("PRAGMA table_info(users)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !columns.iter().any(|c| c == "display_name") {
        db.execute("ALTER TABLE users ADD COLUMN display_name TEXT", [])?;
    }
    Ok(())
}
pub(super) fn validate(name: &str) -> Result<&str> {
    let trimmed = name.trim();
    anyhow::ensure!(
        !trimmed.is_empty() && trimmed.chars().count() <= 80 && !name.chars().any(char::is_control),
        "Use a display name between 1 and 80 characters without control characters"
    );
    Ok(trimmed)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Profile {
    display_name: String,
}
pub(super) async fn update(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Profile>,
) -> ApiResult<Json<User>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.update_profile(&user, &input.display_name).await?,
    ))
}
impl Store {
    pub fn update_profile(&self, user: &User, name: &str) -> Result<User> {
        let name = validate(name)?;
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        let count = tx.execute(
            "UPDATE users SET display_name=?1 WHERE id=?2",
            params![name, user.id],
        )?;
        check(count == 1, "Account unavailable")?;
        tx.execute("INSERT INTO audit(actor,workspace,action,target,at) VALUES(?1,'','profile_updated',?1,?2)", params![user.id, now()])?;
        tx.commit()?;
        Ok(User {
            display_name: Some(name.into()),
            ..user.clone()
        })
    }
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
