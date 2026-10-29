use super::*;
use setup_model::{Activate, Choice, Draft, Edit, Inventory, Preview, Start, Suggestion};
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/setup",
            get(snapshot),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/setup/drafts",
            post(start),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/setup/drafts/{id}",
            post(edit),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/setup/drafts/{id}/preview",
            post(preview),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/setup/drafts/{id}/activate",
            post(activate),
        )
}
async fn snapshot(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let context = app.store.setup_context(&user, &w, &agent).await?;
    let draft = app.store.setup_last_draft(&user, &w, &agent).await?;
    Ok(Json(
        json!({"context":context,"draft":draft,"execution_available":execution_available(&app)?}),
    ))
}
async fn inventories(w: &str, context: &setup_model::Context) -> Result<Vec<Inventory>> {
    let mut result = Vec::new();
    for account in &context.accounts {
        let server = catalog::server(w, &account.server)?;
        ensure!(
            server.fingerprint()? == account.fingerprint,
            "Account configuration changed; reconnect and review it"
        );
        let tools = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            let mut remote = mcp::Remote::connect(server.clone(), &account.fingerprint).await?;
            remote.inventory(&server.operations).await
        })
        .await
        .context("Account discovery timed out")??;
        ensure!(
            catalog::server(w, &account.server)?.fingerprint()? == account.fingerprint,
            "Account changed during discovery"
        );
        let unsupported = tools
            .iter()
            .filter(|name| {
                !account
                    .permissions
                    .iter()
                    .any(|p| p.operation.tool == **name)
            })
            .cloned()
            .collect();
        result.push(Inventory {
            connection: account.id.clone(),
            tools,
            unsupported,
        });
    }
    Ok(result)
}
async fn start(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Start>,
) -> ApiResult<Json<Value>> {
    input.validate()?;
    let user = identity(&app, &headers).await?;
    if let Some(old) = app.store.setup_existing(&user, &w, &agent, &input).await? {
        return Ok(Json(old));
    }
    let context = app
        .store
        .setup_selected_context(&user, &w, &agent, &input)
        .await?;
    let inventories = inventories(&w, &context).await?;
    let choices = context
        .accounts
        .iter()
        .flat_map(|a| {
            a.permissions.iter().map(|p| Choice {
                approver: None,
                approver_revision: None,
                approval_chat: None,
                connection: a.id.clone(),
                operation: p.operation.id.clone(),
                mode: "block".into(),
            })
        })
        .collect();
    let mut draft=Draft { request:input,context,inventories,choices,notes:vec!["This review updates the selected accounts in one conversation. Permissions on other accounts are reviewed separately.".into()],suggestion:None };
    if !draft.request.assistant {
        return Ok(Json(
            app.store.setup_create(&user, &w, &agent, &draft).await?,
        ));
    }
    let provider = draft.context.provider.clone().unwrap_or_default();
    check(
        crate::identity::hosting::executors_available(&app, &provider),
        "AI suggestions need an executor for this agent's model. Use manual review or ask your operator.",
    )?;
    draft.suggestion = Some(Suggestion {
        state: "pending".into(),
        detail: "Preparing AI suggestions. Every action stays blocked until you review it.".into(),
    });
    let suggestion = setup_assistant::context(&draft, &w, &agent);
    let created = app
        .store
        .setup_create_suggested(&user, &w, &agent, &draft, &suggestion)
        .await?;
    app.runtimes.chat_wake.notify_one();
    Ok(Json(created))
}
async fn edit(
    State(app): State<Shared>,
    RoutePath((w, agent, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<Edit>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.setup_edit(&user, &w, &agent, &id, &input).await?,
    ))
}
async fn preview(
    State(app): State<Shared>,
    RoutePath((w, agent, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<Preview>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .setup_preview(&user, &w, &agent, &id, &input)
            .await?,
    ))
}
async fn activate(
    State(app): State<Shared>,
    RoutePath((w, agent, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<Activate>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let saved = app.store.setup_draft(&user, &w, &agent, &id).await?;
    if saved["state"] != "activated" {
        let draft: Draft = serde_json::from_value(saved["data"].clone())?;
        check(
            execution_available(&app)? || draft.choices.iter().all(|c| c.mode == "block"),
            "Business tools require controller-managed execution",
        )?;
        let inventory = inventories(&w, &draft.context).await?;
        check(
            serde_json::to_value(inventory)? == serde_json::to_value(&draft.inventories)?,
            "Tool inventory changed; start a new review",
        )?;
    }
    Ok(Json(
        app.store
            .setup_activate(&user, &w, &agent, &id, &input)
            .await?,
    ))
}
