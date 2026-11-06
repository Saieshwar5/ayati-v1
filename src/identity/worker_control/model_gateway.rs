//! The model gateway (core-agent CA3): executors send model requests here with their
//! assignment capability; the controller picks the assignment's model, adds the
//! provider key it alone holds, streams the answer back and records token use.
use super::*;
use axum::{body::Bytes, response::IntoResponse};
use futures_util::StreamExt;
use serde::Deserialize;

/// Large prompts carry long conversations; 8 MiB bounds one model request.
pub(crate) const MODEL_BODY_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModelRequest {
    assignment: AssignedRequest,
    body: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Provider {
    provider: String,
    base_url: String,
    key_file: std::path::PathBuf,
}

/// The configured providers; read on each request so keys can be rotated in place.
pub(crate) fn provider(file: &std::path::Path, name: &str) -> Result<(String, String)> {
    let providers: Vec<Provider> =
        serde_json::from_slice(&std::fs::read(file).context("Model providers file unavailable")?)
            .context("Invalid model providers file")?;
    let found = providers
        .into_iter()
        .find(|p| p.provider == name)
        .with_context(|| format!("No model provider {name} is configured"))?;
    let url = reqwest::Url::parse(&found.base_url).context("Invalid model provider URL")?;
    ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1")),
        "Model providers need HTTPS; loopback HTTP is for local checks"
    );
    let key =
        std::fs::read_to_string(&found.key_file).context("Model provider key file unavailable")?;
    ensure!(!key.trim().is_empty(), "Model provider key file is empty");
    Ok((
        found.base_url.trim_end_matches('/').to_owned(),
        key.trim().to_owned(),
    ))
}

/// Token counts from a JSON answer or the last usage chunk of a streamed one.
pub(crate) fn usage(body: &[u8]) -> Option<(i64, i64)> {
    let read = |value: &Value| {
        Some((
            value["usage"]["prompt_tokens"].as_i64()?,
            value["usage"]["completion_tokens"].as_i64().unwrap_or(0),
        ))
    };
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        return read(&value);
    }
    String::from_utf8_lossy(body)
        .lines()
        .rev()
        .filter_map(|line| line.trim().strip_prefix("data:"))
        .map(str::trim)
        .filter(|data| *data != "[DONE]")
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .find_map(|value| read(&value))
}

pub(crate) fn upstream_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("model gateway client")
    })
}

/// Saves each call the gateway makes, for usage, spend and provider health.
#[derive(Clone)]
pub(crate) struct Recorder {
    pub store: super::super::postgres::PgStore,
    pub scope: Scope,
    pub job: Option<String>,
    /// The model the agent asked for, when a route chose the target.
    pub requested: Option<String>,
}
impl Recorder {
    async fn save(
        &self,
        target: &super::model_routing::Target,
        started: std::time::Instant,
        ok: bool,
        fell_back: bool,
        tokens: (i64, i64),
    ) {
        self.save_named(
            &target.provider,
            &target.model,
            started,
            ok,
            fell_back,
            tokens,
        )
        .await;
    }
    pub(crate) async fn save_named(
        &self,
        provider: &str,
        model: &str,
        started: std::time::Instant,
        ok: bool,
        fell_back: bool,
        tokens: (i64, i64),
    ) {
        let saved = self
            .store
            .record_model_call(&super::super::postgres::ModelCall {
                workspace: &self.scope.workspace,
                assignment: &self.scope.assignment,
                agent: &self.scope.agent,
                job: self.job.as_deref(),
                provider,
                model,
                requested_model: self.requested.as_deref(),
                input_tokens: tokens.0,
                output_tokens: tokens.1,
                latency_ms: Some(started.elapsed().as_millis() as i64),
                ok,
                fell_back,
            })
            .await;
        if let Err(error) = saved {
            tracing::warn!(error = format!("{error:#}"), "Model usage was not saved");
        }
    }
}

/// An OpenAI-style error the agent's runtime shows instead of an answer.
fn refused(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(json!({"error":{"message": message, "type": "cuckoo_refused"}})),
    )
        .into_response()
}

/// Whether the next target should get this request: the provider could not be
/// reached, was too slow to start answering, is overloaded or failed itself.
fn retryable(status: Option<reqwest::StatusCode>) -> bool {
    status.is_none_or(|s| s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS)
}

/// Tries each target in order until one answers. A target that cannot be
/// reached, is slower than the route's timeout to start answering, is
/// overloaded or fails itself is recorded as an error and the next one gets the
/// request. The last target's answer is returned whatever it is.
pub(crate) async fn first_answer(
    plan: &super::model_routing::Plan,
    body: &mut Value,
    recorder: &Recorder,
) -> std::result::Result<(reqwest::Response, usize, std::time::Instant), Response> {
    let last = plan.targets.len() - 1;
    for (index, target) in plan.targets.iter().enumerate() {
        body["model"] = json!(target.model);
        let started = std::time::Instant::now();
        let sent = upstream_client()
            .post(format!("{}/chat/completions", target.base_url))
            .bearer_auth(&target.key)
            .json(&*body)
            .timeout(std::time::Duration::from_secs(600))
            .send();
        let result = match tokio::time::timeout(plan.timeout, sent).await {
            Ok(Ok(response)) => Some(response),
            _ => None,
        };
        let status = result.as_ref().map(|r| r.status());
        match result {
            Some(response) if index == last || !retryable(status) => {
                return Ok((response, index, started));
            }
            _ => {
                recorder
                    .save(target, started, false, index > 0, (0, 0))
                    .await
            }
        }
    }
    Err(refused(
        StatusCode::BAD_GATEWAY,
        "The model provider could not be reached",
    ))
}

pub(super) async fn invoke(
    State(state): State<StateData>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<ModelRequest>,
) -> std::result::Result<Response, Error> {
    super::super::postgres::worker_control::require(id == input.assignment.scope.assignment)?;
    let hash = credential(&headers)?;
    let record = pg(&state)
        .controlled_assignment(&input.assignment.scope, &hash)
        .await?;
    super::super::postgres::worker_control::require(
        record.session.as_deref() == Some(&input.assignment.session),
    )?;
    let context = record.context.clone().unwrap_or(Value::Null);
    // The assignment's model is authoritative, whatever Goose asked for.
    let requested = context["model"]
        .as_str()
        .context("Assignment has no model")?
        .to_owned();
    let assigned_provider = context["provider"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| state.app.config.worker_providers.first().cloned())
        .unwrap_or_else(|| "openai".into());
    let mut body = input.body;
    if !body.is_object() {
        return Err(anyhow::anyhow!("Invalid model request").into());
    }
    if body["stream"] == true {
        body["stream_options"] = json!({"include_usage": true});
    }
    let job = match &record.work {
        super::super::postgres::worker_control::Work::Reply(assignment) => {
            Some(assignment.job.reply.clone())
        }
        super::super::postgres::worker_control::Work::Task(_) => None,
    };
    let scope = input.assignment.scope.clone();
    let sticky = job.clone().unwrap_or_else(|| scope.assignment.clone());
    let plan = match super::model_routing::plan(
        &state.app,
        &scope.workspace,
        &assigned_provider,
        &requested,
        &sticky,
    )
    .await?
    {
        Ok(plan) => plan,
        Err(super::model_routing::Refused(message)) => {
            return Ok(refused(StatusCode::FORBIDDEN, &message));
        }
    };
    let recorder = Recorder {
        store: state.app.store.clone(),
        scope: scope.clone(),
        job: job.clone(),
        requested: plan.routed.then(|| requested.clone()),
    };
    let (response, index, started) = match first_answer(&plan, &mut body, &recorder).await {
        Ok(answer) => answer,
        Err(refusal) => return Ok(refusal),
    };
    let target = &plan.targets[index];
    let status = response.status();
    let kind = response.headers().get(header::CONTENT_TYPE).cloned();
    // Forward as it arrives; keep the tail to read token counts when it ends.
    let (sender, receiver) =
        tokio::sync::mpsc::channel::<std::result::Result<Bytes, std::io::Error>>(32);
    let (provider, model, fell_back) = (target.provider.clone(), target.model.clone(), index > 0);
    tokio::spawn(async move {
        let mut stream = response.bytes_stream();
        let mut tail: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else { break };
            tail.extend_from_slice(&chunk);
            if tail.len() > 65536 {
                tail.drain(..tail.len() - 65536);
            }
            if sender.send(Ok(chunk)).await.is_err() {
                break;
            }
        }
        let tokens = if status.is_success() {
            usage(&tail).unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        recorder
            .save_named(
                &provider,
                &model,
                started,
                status.is_success(),
                fell_back,
                tokens,
            )
            .await;
    });
    let mut built = Response::builder().status(status.as_u16());
    if let Some(kind) = kind {
        built = built.header(header::CONTENT_TYPE, kind);
    }
    Ok(built
        .body(Body::from_stream(
            tokio_stream::wrappers::ReceiverStream::new(receiver),
        ))
        .map_err(anyhow::Error::from)?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn usage_comes_from_json_or_the_last_usage_chunk() {
        assert_eq!(
            super::usage(br#"{"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":3}}"#),
            Some((12, 3))
        );
        let sse = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":40,\"completion_tokens\":7}}\n\ndata: [DONE]\n\n";
        assert_eq!(super::usage(sse), Some((40, 7)));
        assert_eq!(super::usage(b"data: {\"choices\":[]}\n\n"), None);
    }
}
