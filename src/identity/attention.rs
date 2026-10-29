//! Personal read receipts refer to saved work; they never authorize that work.
use super::*;
use axum::{Json, extract::Path as RoutePath, http::HeaderMap};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cursor {
    pub at: i64,
    pub kind: String,
    pub id: String,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Page {
    #[serde(default)]
    pub view: String,
    pub before: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Acknowledge {
    pub kind: String,
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub reviewed: bool,
}
impl Page {
    pub(crate) fn cursor(&self) -> Result<Cursor> {
        anyhow::ensure!(
            ["", "needs_you", "recent"].contains(&self.view.as_str()),
            "Invalid attention view"
        );
        let Some(before) = &self.before else {
            return Ok(Cursor {
                at: i64::MAX,
                kind: "z".into(),
                id: "z".into(),
            });
        };
        anyhow::ensure!(before.len() <= 512, "Invalid attention cursor");
        let cursor: Cursor = serde_json::from_str(before).context("Invalid attention cursor")?;
        anyhow::ensure!(cursor.at >= 0, "Invalid attention cursor");
        validate(&cursor.kind, &cursor.id)?;
        Ok(cursor)
    }
}
impl Acknowledge {
    pub(crate) fn validate(&self) -> Result<()> {
        validate(&self.kind, &self.id)?;
        anyhow::ensure!(
            !self.version.is_empty()
                && self.version.len() <= 96
                && !self.version.chars().any(char::is_control),
            "Invalid attention version"
        );
        Ok(())
    }
}
fn validate(kind: &str, id: &str) -> Result<()> {
    anyhow::ensure!(
        ["question", "approval", "outcome"].contains(&kind) && uuid::Uuid::parse_str(id).is_ok(),
        "Invalid attention source"
    );
    Ok(())
}
pub(super) fn routes() -> Router<Shared> {
    Router::new().route(
        "/api/identity/workspaces/{w}/attention",
        get(list).post(acknowledge),
    )
}
async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    axum::extract::Query(input): axum::extract::Query<Page>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &w).await?;
    input.cursor()?;
    let pg = &app.store;
    Ok(Json(pg.attention(&user, &w, &input).await?))
}
async fn acknowledge(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<Acknowledge>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let pg = &app.store;
    Ok(Json(pg.acknowledge_attention(&user, &w, &input).await?))
}
