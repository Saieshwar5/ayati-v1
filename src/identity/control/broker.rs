//! Each Goose invocation gets a fresh loopback-only capability, bound to its exact job.
use super::model::ActionRequest;
use crate::identity::{
    chat::model::Job,
    postgres::PgStore,
    worker_control::{client::Client, model::AssignedRequest},
};
use anyhow::{Result, ensure};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone)]
pub(crate) struct Endpoint {
    pub address: String,
    pub token: String,
}
/// The executor's controller session for one assignment. Tool calls are
/// forwarded to the controller, which alone holds database and provider access.
#[derive(Clone)]
pub(crate) struct Access {
    pub client: Client,
    pub assignment: AssignedRequest,
}
pub(crate) struct Broker {
    task: tokio::task::JoinHandle<()>,
    pub endpoint: Endpoint,
}
impl Drop for Broker {
    fn drop(&mut self) {
        self.task.abort();
    }
}
#[derive(Clone)]
struct BrokerState {
    access: Access,
    token: String,
    host: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    token: String,
    tool: String,
    arguments: Value,
}
impl Broker {
    pub async fn start(access: Access) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?.to_string();
        let token = crate::identity::provider::random_token();
        let router = Router::new()
            .route("/control", post(exchange))
            .layer(axum::extract::DefaultBodyLimit::max(24576))
            .with_state(BrokerState {
                access,
                token: token.clone(),
                host: address.clone(),
            });
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self {
            task,
            endpoint: Endpoint { address, token },
        })
    }
}
async fn exchange(
    State(state): State<BrokerState>,
    headers: HeaderMap,
    Json(input): Json<Request>,
) -> (StatusCode, Json<Value>) {
    let result = async {
        ensure!(
            headers.get("host").and_then(|h| h.to_str().ok()) == Some(&state.host)
                && !headers.contains_key("origin")
                && !headers.contains_key("cookie")
                && input.token == state.token,
            "Tool capability denied"
        );
        let Access { client, assignment } = &state.access;
        let seconds = tool_seconds(&input.tool, &input.arguments);
        client
            .post_timeout::<Value>(
                &format!("/v1/assignments/{}/tools", assignment.scope.assignment),
                &RemoteRequest {
                    assignment: assignment.clone(),
                    tool: input.tool,
                    arguments: input.arguments,
                },
                seconds,
            )
            .await
    }
    .await;
    match result {
        Ok(value) => (StatusCode::OK, Json(json!({"result":value}))),
        Err(_) => (
            StatusCode::FORBIDDEN,
            Json(
                json!({"error":"Operation denied or unavailable. Check the saved action history before retrying."}),
            ),
        ),
    }
}
/// How long the controller may take to answer one tool call. A command on the agent's
/// computer may run for its own `seconds`; other computer and browser work gets a
/// minute and a half. The guest waits a little longer (`computer_schema::seconds`).
fn tool_seconds(tool: &str, arguments: &Value) -> u64 {
    match tool {
        "computer_run" | "run_skill_script" => {
            arguments["seconds"].as_u64().unwrap_or(60).clamp(1, 600) + 30
        }
        _ if tool.starts_with("computer_") || tool.starts_with("browser_") => 90,
        _ => 45,
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteRequest {
    pub assignment: AssignedRequest,
    pub tool: String,
    pub arguments: Value,
}
pub(crate) async fn invoke(pg: &PgStore, job: &Job, tool: &str, arguments: Value) -> Result<Value> {
    let mut result = invoke_tool(pg, job, tool, arguments).await?;
    // Steering (Phase 4 AC6): what the person added to this running request reaches
    // the agent with the result of its next step.
    let added = pg.take_steers(job).await?;
    if !added.is_empty() {
        match result.as_object_mut() {
            Some(fields) => {
                fields.insert("person_added".into(), json!(added));
            }
            None => result = json!({"result": result, "person_added": added}),
        }
    }
    Ok(result)
}
async fn invoke_tool(pg: &PgStore, job: &Job, tool: &str, arguments: Value) -> Result<Value> {
    pg.home_maintenance_tool(job, tool).await?;
    match tool {
        "home_panel_update_context" => {
            ensure!(
                arguments == json!({}),
                "Invalid panel update context request"
            );
            pg.home_update_context(job).await
        }
        "refresh_home_panel" => {
            pg.refresh_home_panel(job, &serde_json::from_value(arguments)?)
                .await
        }
        "save_working_context" => {
            let input = serde_json::from_value(arguments)?;
            pg.save_working_context(job, &input).await
        }
        "search_conversation_history" => {
            let input = serde_json::from_value(arguments)?;
            pg.search_conversation_history(job, &input).await
        }
        "read_conversation_history" => {
            let input = serde_json::from_value(arguments)?;
            pg.read_conversation_history(job, &input).await
        }
        "ask_question" => {
            let parsed =
                serde_json::from_value::<crate::identity::interactions::model::Ask>(arguments);
            let question = match parsed {
                Ok(question) => question,
                Err(error) => {
                    return Ok(
                        json!({"state":"invalid","message":format!("Invalid question specification: {error}").chars().take(400).collect::<String>()}),
                    );
                }
            };
            if let Err(error) = question.validate() {
                return Ok(json!({"state":"invalid","message":error.to_string()}));
            }
            pg.ask_question(job, &question).await
        }
        "personal_panels" => {
            ensure!(arguments == json!({}), "Invalid personal panel discovery");
            pg.personal_panels(job).await
        }
        "save_home_panel" => {
            let input = serde_json::from_value(arguments)?;
            pg.save_personal_panel(job, &input).await
        }
        "handoff_targets" => {
            ensure!(arguments == json!({}), "Invalid helper discovery request");
            pg.handoff_targets(job).await
        }
        "request_handoff" => {
            let input = serde_json::from_value(arguments)?;
            pg.request_handoff(job, &input).await
        }
        "continue_privately" => {
            ensure!(
                arguments == json!({}),
                "Invalid private continuation request"
            );
            pg.continue_privately(job).await
        }
        "send_update" => {
            let input = serde_json::from_value(arguments)?;
            pg.send_update(job, &input).await
        }
        "load_skill" => {
            let input = serde_json::from_value(arguments)?;
            pg.load_skill(job, &input).await
        }
        "create_task" => {
            pg.create_task(job, &serde_json::from_value(arguments)?)
                .await
        }
        "attach_to_task" => {
            pg.attach_to_task(job, &serde_json::from_value(arguments)?)
                .await
        }
        "open_tasks" => {
            ensure!(arguments == json!({}), "Invalid open tasks request");
            pg.open_tasks(job).await
        }
        "close_task" => {
            pg.close_task(job, &serde_json::from_value(arguments)?)
                .await
        }
        "follow_up" => pg.follow_up(job, &serde_json::from_value(arguments)?).await,
        "my_goals" => {
            ensure!(arguments == json!({}), "Invalid goals request");
            pg.my_goals(job).await
        }
        "update_goal" => {
            pg.update_goal(job, &serde_json::from_value(arguments)?)
                .await
        }
        "write_review" => {
            pg.write_review(job, &serde_json::from_value(arguments)?)
                .await
        }
        "propose_schedule" => {
            pg.propose_schedule(job, &serde_json::from_value(arguments)?)
                .await
        }
        "propose_rule" => {
            let Ok(proposal) = serde_json::from_value(arguments) else {
                return Ok(json!({"state":"invalid","message":"Invalid rule proposal"}));
            };
            pg.propose_guard_rule(job, &proposal).await
        }
        "list_folder" => {
            pg.library_list_folder(job, &serde_json::from_value(arguments)?)
                .await
        }
        "search_library" => {
            pg.library_search(job, &serde_json::from_value(arguments)?)
                .await
        }
        "read_document" => {
            pg.library_read_document(job, &serde_json::from_value(arguments)?)
                .await
        }
        "suggest_edit" => {
            pg.library_suggest_edit(job, &serde_json::from_value(arguments)?)
                .await
        }
        "edit_document" => {
            pg.library_edit_document(job, &serde_json::from_value(arguments)?)
                .await
        }
        "save_document" => {
            pg.library_save_document(job, &serde_json::from_value(arguments)?)
                .await
        }
        "file_document" => {
            pg.library_file_document(job, &serde_json::from_value(arguments)?)
                .await
        }
        "set_document_fields" => {
            pg.library_set_fields(job, &serde_json::from_value(arguments)?)
                .await
        }
        "lookup" => {
            pg.library_lookup(job, &serde_json::from_value(arguments)?)
                .await
        }
        "set_table_row" => {
            pg.library_set_row(job, &serde_json::from_value(arguments)?)
                .await
        }
        "web_search" => {
            pg.web_search(job, &serde_json::from_value(arguments)?)
                .await
        }
        "read_web_page" => {
            pg.read_web_page(job, &serde_json::from_value(arguments)?)
                .await
        }
        "computer_run" => {
            pg.computer_run(job, &serde_json::from_value(arguments)?)
                .await
        }
        "run_skill_script" => {
            pg.run_skill_script(job, &serde_json::from_value(arguments)?)
                .await
        }
        "computer_list_files" => {
            pg.computer_list_files(job, &serde_json::from_value(arguments)?)
                .await
        }
        "computer_read_file" => {
            pg.computer_read_file(job, &serde_json::from_value(arguments)?)
                .await
        }
        "computer_write_file" => {
            pg.computer_write_file(job, &serde_json::from_value(arguments)?)
                .await
        }
        "browser_open" => {
            pg.browser_open(job, &serde_json::from_value(arguments)?)
                .await
        }
        "browser_read" => {
            ensure!(arguments == json!({}), "Invalid browser request");
            pg.browser_read(job).await
        }
        "browser_click" => {
            pg.browser_click(job, &serde_json::from_value(arguments)?)
                .await
        }
        "browser_type" => {
            pg.browser_type(job, &serde_json::from_value(arguments)?)
                .await
        }
        "browser_choose" => {
            pg.browser_choose(job, &serde_json::from_value(arguments)?)
                .await
        }
        "browser_press" => {
            pg.browser_press(job, &serde_json::from_value(arguments)?)
                .await
        }
        "browser_screenshot" => {
            pg.browser_screenshot(job, &serde_json::from_value(arguments)?)
                .await
        }
        "browser_hand_over" => {
            pg.browser_hand_over(job, &serde_json::from_value(arguments)?)
                .await
        }
        "my_apps" => {
            ensure!(arguments == json!({}), "Invalid apps request");
            pg.agent_my_apps(job).await
        }
        "app_files" => {
            pg.agent_app_files(job, &serde_json::from_value(arguments)?)
                .await
        }
        "read_app_file" => {
            pg.agent_read_app_file(job, &serde_json::from_value(arguments)?)
                .await
        }
        "app_actions" => {
            ensure!(arguments == json!({}), "Invalid app actions request");
            pg.agent_app_actions(job).await
        }
        "run_app_action" => {
            pg.agent_run_app_action(job, &serde_json::from_value(arguments)?)
                .await
        }
        "write_app_file" => {
            pg.agent_write_app_file(job, &serde_json::from_value(arguments)?)
                .await
        }
        "report_problem" => {
            pg.report_agent_need(job, &serde_json::from_value(arguments)?)
                .await
        }
        "operations" => {
            ensure!(arguments == json!({}), "Invalid discovery request");
            pg.control_operations(job).await
        }
        "request_action" => {
            let request: ActionRequest = serde_json::from_value(arguments)?;
            let action = pg.control_propose(job, &request).await?;
            super::dispatch(pg, &job.workspace, action).await
        }
        _ => anyhow::bail!("Unsupported control operation"),
    }
}
