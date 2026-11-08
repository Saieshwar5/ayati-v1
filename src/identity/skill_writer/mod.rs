//! Skills written in chat (designs/agent-built-skills 2 and 5). While people set
//! up an agent, a model reads what they describe and writes Ground rules and
//! skills as drafts. It has no tools and nothing is saved here: the browser shows
//! the drafts and saves them only when the person does, through the ordinary
//! skill routes and their checks. Needs `AYATI_SKILL_WRITER_MODEL`.
mod answer;
#[cfg(test)]
mod answer_tests;
pub(crate) mod controls;
#[cfg(test)]
mod controls_tests;
pub(crate) mod guide;
#[cfg(test)]
mod request_tests;

use super::*;
use crate::identity::agents::config::INSTRUCTIONS_LIMIT;
use anyhow::{Context, Result, ensure};
use axum::{Json, extract::Path as RoutePath, http::HeaderMap};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const TIME_LIMIT: Duration = Duration::from_secs(90);
const NOT_REACHED: &str =
    "The skill writer couldn't answer. Try again, or write the skill yourself.";
const NOT_SET_UP: &str = "Writing skills in chat isn't set up here. Write the ground rules and skills yourself, or ask your operator to set AYATI_SKILL_WRITER_MODEL.";

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/skill-writer",
            get(status).post(write),
        )
        .route(
            "/api/identity/workspaces/{w}/skill-writer/controls",
            post(controls::suggest),
        )
}

/// One message of the setup chat: the person's or the writer's.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Message {
    /// "person" or "agent".
    pub role: String,
    pub text: String,
}
/// A skill the agent already has as a draft or saved.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftIn {
    pub name: String,
    pub description: String,
    pub steps: String,
    /// Saved skills keep their name; the writer is told so.
    #[serde(default)]
    pub saved: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub name: String,
    pub role: String,
    /// A saved agent being changed; only its owner or an admin may.
    #[serde(default)]
    pub agent: Option<String>,
    /// For a new agent: the connectors chosen in step 2.
    #[serde(default)]
    pub connections: Vec<String>,
    pub ground_rules: String,
    #[serde(default)]
    pub skills: Vec<DraftIn>,
    pub messages: Vec<Message>,
    /// A skill the person asked about ("Ask Iris").
    #[serde(default)]
    pub focus: Option<String>,
}
impl Request {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.name.len() <= 80 && self.role.chars().count() <= 80,
            "The name or job title is too long"
        );
        ensure!(
            self.ground_rules.len() <= INSTRUCTIONS_LIMIT,
            "The ground rules are too long"
        );
        ensure!(
            self.skills.len() <= 20
                && self.skills.iter().all(|s| s.name.len() <= 64
                    && s.description.len() <= 1024
                    && s.steps.len() <= 20_000),
            "There are too many skills, or one is too long"
        );
        ensure!(
            (1..=40).contains(&self.messages.len())
                && self
                    .messages
                    .iter()
                    .all(|m| ["person", "agent"].contains(&m.role.as_str())
                        && !m.text.trim().is_empty()
                        && m.text.len() <= 8000),
            "Send 1 to 40 messages of up to 8,000 characters"
        );
        ensure!(
            self.messages.last().is_some_and(|m| m.role == "person"),
            "The last message must be yours"
        );
        ensure!(self.connections.len() <= 50, "Too many apps");
        ensure!(
            self.focus.as_ref().is_none_or(|f| f.len() <= 64),
            "Unknown skill"
        );
        Ok(())
    }
}

/// A library skill the person may add to this agent.
pub(crate) struct LibrarySkill {
    pub id: String,
    pub name: String,
    pub description: String,
}
/// What the writer knows besides the conversation, read under the person's access.
#[derive(Default)]
pub(crate) struct WriterContext {
    /// Every skill name in the workspace, so new names never collide.
    pub names: Vec<String>,
    /// Workspace skills and the person's own, for reuse.
    pub library: Vec<LibrarySkill>,
    /// The agent's apps: `{name, actions: [{title, kind}]}`.
    pub apps: Vec<Value>,
}

/// The controller's skill writer model, from its configuration.
pub(crate) struct Configured {
    pub file: std::path::PathBuf,
    pub provider: String,
    pub model: String,
}
impl Configured {
    pub fn from(config: &crate::config::Controller) -> Option<Self> {
        let (provider, model) = config.skill_writer_model.clone()?;
        Some(Self {
            file: config.model_providers_file.clone()?,
            provider,
            model,
        })
    }
}

/// The model's text and the tokens it used.
pub(crate) struct Reply {
    pub content: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// One chat completion asking for a JSON object. The key is read from the
/// providers file for this call and never logged.
pub(crate) async fn ask(
    configured: &Configured,
    messages: Vec<Value>,
    limit: Duration,
) -> Result<Reply> {
    let (base, key) =
        crate::identity::worker_control::model_provider(&configured.file, &configured.provider)
            .context(NOT_REACHED)?;
    let response = crate::identity::worker_control::model_client()
        .post(format!("{base}/chat/completions"))
        .bearer_auth(key)
        .json(&json!({"model": configured.model, "messages": messages,
            "response_format": {"type": "json_object"}, "max_tokens": 6000}))
        .timeout(limit)
        .send()
        .await
        .context(NOT_REACHED)?;
    let status = response.status();
    let value: Value = response.json().await.context(NOT_REACHED)?;
    ensure!(status.is_success(), "{NOT_REACHED} ({status})");
    Ok(Reply {
        content: value["choices"][0]["message"]["content"]
            .as_str()
            .context(NOT_REACHED)?
            .to_owned(),
        input_tokens: value["usage"]["prompt_tokens"].as_i64().unwrap_or(0),
        output_tokens: value["usage"]["completion_tokens"].as_i64().unwrap_or(0),
    })
}

/// Whether skills can be written in chat here, and the writer's guide.
async fn status(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &w).await?;
    Ok(Json(json!({
        "available": Configured::from(&app.config).is_some(),
        "guide": guide::GUIDE,
    })))
}

/// One turn of the setup chat: the writer's reply and drafts. Nothing is saved.
async fn write(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<Request>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &w).await?;
    input.validate()?;
    if let Some(agent) = &input.agent {
        app.store.managed_agent(&user, &w, agent, true).await?;
    }
    let configured = Configured::from(&app.config).context(NOT_SET_UP)?;
    let context = app
        .store
        .skill_writer_context(&user, &w, input.agent.as_deref(), &input.connections)
        .await?;
    let started = Instant::now();
    let reply = ask(&configured, guide::messages(&input, &context), TIME_LIMIT).await?;
    let usage = app
        .store
        .record_model_usage(&crate::identity::postgres::ModelCall {
            workspace: &w,
            assignment: "skill_writer",
            agent: input.agent.as_deref().unwrap_or(""),
            job: None,
            provider: &configured.provider,
            model: &configured.model,
            provider_model: &configured.model,
            attempts: 1,
            input_tokens: reply.input_tokens,
            output_tokens: reply.output_tokens,
            person: Some(&user.id),
            charge_micro: 0,
            cost_micro: 0,
            latency_ms: i64::try_from(started.elapsed().as_millis()).ok(),
        })
        .await;
    if usage.is_err() {
        tracing::warn!("Skill writer usage could not be recorded");
    }
    let answer = answer::read(&reply.content, &input, &context)?;
    Ok(Json(serde_json::to_value(answer)?))
}
