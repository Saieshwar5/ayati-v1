//! Each Goose invocation gets a fresh loopback-only capability, bound to its exact job.
use super::model::ActionRequest;
use crate::identity::{
    chat::store::Job,
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
#[derive(Clone)]
pub(crate) enum Access {
    Local(PgStore, Job),
    Remote(Client, AssignedRequest),
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
        match &state.access {
            Access::Local(pg, job) => invoke(pg, job, &input.tool, input.arguments).await,
            Access::Remote(client, assignment) => {
                client
                    .post_timeout(
                        &format!("/v1/assignments/{}/tools", assignment.scope.assignment),
                        &RemoteRequest {
                            assignment: assignment.clone(),
                            tool: input.tool,
                            arguments: input.arguments,
                        },
                        45,
                    )
                    .await
            }
        }
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
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteRequest {
    pub assignment: AssignedRequest,
    pub tool: String,
    pub arguments: Value,
}
pub(crate) async fn invoke(pg: &PgStore, job: &Job, tool: &str, arguments: Value) -> Result<Value> {
    match tool {
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
