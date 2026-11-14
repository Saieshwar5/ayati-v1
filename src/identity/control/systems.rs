//! Routes for connected systems (C1 of the connected systems plan, D249): an
//! agent's owner or an admin connects the company's own MCP server from the
//! agent's Tools tab, reviews each tool, and changes or removes it later.
use super::*;
use crate::identity::connectors::{
    system_tools::Reviewed as Tool,
    systems::{self, Address},
};
use crate::identity::postgres::systems::{NewSystem, Reviewed, SystemReview, ToolChoice};
use model::Operation;

pub(super) fn routes() -> Router<Shared> {
    let base = "/api/identity/workspaces/{w}/agent-profiles/{agent}/systems";
    Router::new()
        .route(base, get(list).post(create))
        .route(&format!("{base}/preview"), post(preview))
        .route(&format!("{base}/{{id}}/review"), post(review))
        .route(&format!("{base}/{{id}}/tools"), post(saved_tools))
        .route(&format!("{base}/{{id}}/state"), post(state))
}

async fn list(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.agent_systems(&user, &w, &agent).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preview {
    url: String,
    #[serde(default)]
    key: Option<String>,
}

/// Step 1: reach the address and list its tools for review. Nothing is saved.
async fn preview(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Preview>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    owner_or_admin(&app, &user, &w, &agent).await?;
    let address = Address {
        url: input.url.trim().to_owned(),
        key: input.key.filter(|k| !k.trim().is_empty()),
    };
    let tools = systems::list(&w, &address).await?;
    Ok(Json(
        json!({"tools":tools.iter().map(Tool::public).collect::<Vec<_>>()}),
    ))
}

async fn owner_or_admin(app: &App, user: &User, w: &str, agent: &str) -> Result<()> {
    let listed = app.store.agent_systems(user, w, agent).await?;
    ensure!(
        listed["can_connect"] == true,
        "Only the agent's owner or an admin can connect or change its systems"
    );
    Ok(())
}

/// The reviewed actions for the person's choices, checked against the tools
/// the system lists now: a tool that changed since the screen showed it, or
/// that Cuckoo can't use, is refused with its name.
fn operations(listed: &[Tool], chosen: &[ToolChoice]) -> Result<Vec<Operation>> {
    chosen
        .iter()
        .map(|choice| {
            listed
                .iter()
                .find(|t| t.name == choice.name)
                .and_then(|t| t.operation(&choice.kind))
                .with_context(|| {
                    format!(
                        "The tool \u{201c}{}\u{201d} isn't offered in a form Cuckoo can use. Review the tools again.",
                        choice.name
                    )
                })
        })
        .collect()
}

/// Step 2: save the reviewed system, its rules and the agent's switch.
async fn create(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(mut input): Json<NewSystem>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(
        execution_available(&app)?,
        "Business tools need controller-managed execution; ask your operator to enable it",
    )?;
    input.validate()?;
    owner_or_admin(&app, &user, &w, &agent).await?;
    input.url = input.url.trim().to_owned();
    input.key = input.key.filter(|k| !k.trim().is_empty());
    let address = Address {
        url: input.url.clone(),
        key: input.key.clone(),
    };
    let listed = systems::list(&w, &address).await?;
    let reviewed = Reviewed {
        operations: operations(&listed, &input.tools)?,
        binding: address.binding(&w, &systems::server_name(&input.id))?,
    };
    Ok(Json(
        app.store
            .connect_system(&user, &w, &agent, &input, &reviewed)
            .await?,
    ))
}

/// The saved system's tools as it lists them now, with its saved key.
async fn saved_tools(
    State(app): State<Shared>,
    RoutePath((w, agent, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let (server, binding) = app.store.system_binding(&user, &w, &agent, &id).await?;
    let tools = systems::list_saved(&w, &server, &binding).await?;
    Ok(Json(
        json!({"tools":tools.iter().map(Tool::public).collect::<Vec<_>>()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    revision: i64,
    tools: Vec<ToolChoice>,
    /// A replacement key; the saved one is kept when this is absent.
    #[serde(default)]
    key: Option<String>,
}

/// Saves a new review of the tools, and optionally a new key.
async fn review(
    State(app): State<Shared>,
    RoutePath((w, agent, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<Review>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let (server, binding) = app.store.system_binding(&user, &w, &agent, &id).await?;
    let (listed, binding) = match input.key.filter(|k| !k.trim().is_empty()) {
        Some(key) => {
            let address = Address {
                url: binding["url"].as_str().unwrap_or_default().to_owned(),
                key: Some(key),
            };
            (
                systems::list(&w, &address).await?,
                address.binding(&w, &server)?,
            )
        }
        None => (systems::list_saved(&w, &server, &binding).await?, binding),
    };
    let reviewed = Reviewed {
        operations: operations(&listed, &input.tools)?,
        binding,
    };
    Ok(Json(
        app.store
            .review_system(
                &user,
                &w,
                &agent,
                &id,
                &SystemReview {
                    revision: input.revision,
                    tools: input.tools,
                },
                &reviewed,
            )
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    revision: i64,
    /// `on`, `off` or `remove`.
    change: String,
}

async fn state(
    State(app): State<Shared>,
    RoutePath((w, agent, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<Change>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .change_system(&user, &w, &agent, &id, input.revision, &input.change)
            .await?,
    ))
}
