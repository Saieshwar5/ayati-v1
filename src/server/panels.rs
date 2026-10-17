use super::{ApiResult, WebState};
use anyhow::Context;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/panels", get(list))
        .route("/api/panels/{id}/activate", post(activate))
        .route("/api/panels/{id}/remove", post(remove))
        .route("/api/panels/{id}/updates", post(updates))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    revision: i64,
    updates_enabled: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    revision: i64,
}
async fn list(State(state): State<WebState>) -> ApiResult<Json<Value>> {
    let Some(platform) = state.agent.platform() else {
        return Ok(Json(json!({"panels":[],"available":false})));
    };
    let mut panels = platform.panels(None)?;
    if let Some(workspace) = &state.workspace {
        for panel in panels.as_array_mut().unwrap() {
            let mut maintenance = Vec::new();
            for (agent, id) in platform.panel_schedules(panel["id"].as_str().unwrap())? {
                let (agent, _, _) = workspace.agent(&agent)?;
                let schedule = agent.store.schedules()?.into_iter().find(|s| s.id == id);
                let runs = agent.store.schedule_runs(&id)?;
                maintenance.push(json!({"schedule":schedule,"latest_run":runs.first()}));
            }
            panel["maintenance"] = json!(maintenance);
        }
    }
    Ok(Json(json!({"panels":panels,"available":true})))
}
fn set_bound_schedules(state: &WebState, id: &str, enabled: bool) -> anyhow::Result<()> {
    let p = state.agent.platform().context("Home panels unavailable")?;
    let Some(workspace) = &state.workspace else {
        return Ok(());
    };
    for (agent, id) in p.panel_schedules(id)? {
        let (agent, _, _) = workspace.agent(&agent)?;
        if agent
            .store
            .schedules()?
            .iter()
            .any(|s| s.id == id && s.state != "completed")
        {
            agent.set_schedule_enabled(&id, enabled)?;
        }
    }
    Ok(())
}
async fn activate(
    State(state): State<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Review>,
) -> ApiResult<Json<Value>> {
    state
        .agent
        .platform()
        .context("Home panels require Goose")?
        .activate_panel(&id, input.revision, input.updates_enabled)
        .await?;
    set_bound_schedules(&state, &id, input.updates_enabled)?;
    Ok(Json(json!({"ok":true})))
}
async fn remove(
    State(state): State<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Revision>,
) -> ApiResult<Json<Value>> {
    state
        .agent
        .platform()
        .context("Home panels require Goose")?
        .remove_panel(&id, input.revision)
        .await?;
    set_bound_schedules(&state, &id, false)?;
    Ok(Json(json!({"ok":true})))
}
async fn updates(
    State(state): State<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Review>,
) -> ApiResult<Json<Value>> {
    state
        .agent
        .platform()
        .context("Home panels require Goose")?
        .set_panel_updates(&id, input.revision, input.updates_enabled)
        .await?;
    set_bound_schedules(&state, &id, input.updates_enabled)?;
    Ok(Json(json!({"ok":true})))
}
