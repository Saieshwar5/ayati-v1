//! The model gateway (core-agent CA3): executors send model requests here with their
//! assignment capability; the controller picks the assignment's model, adds the
//! provider key it alone holds, streams the answer back and records token use.
use super::*;
use crate::identity::models::catalog::Price;
use crate::identity::postgres::model_usage_micro_dollars as micro_dollars;
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
    /// The API the provider speaks; only OpenAI's chat completions for now.
    #[serde(default = "openai")]
    protocol: String,
}
fn openai() -> String {
    "openai".into()
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
    ensure!(
        found.protocol == "openai",
        "Model provider {name} speaks an API Cuckoo can't use yet"
    );
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
    let file = state
        .app
        .config
        .model_providers_file
        .clone()
        .context("The model gateway is off")?;
    let context = record.context.clone().unwrap_or(Value::Null);
    // The assignment's model is authoritative, whatever Goose asked for.
    let model = context["model"]
        .as_str()
        .context("Assignment has no model")?
        .to_owned();
    let name = context["provider"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| state.app.config.worker_providers.first().cloned())
        .unwrap_or_else(|| "openai".into());
    // A used-up monthly budget stops every model call, whatever started the work (MG3).
    if let Some(reason) = pg(&state)
        .model_budget_reached(
            &input.assignment.scope.workspace,
            &input.assignment.scope.agent,
        )
        .await?
    {
        return Ok((
            StatusCode::PAYMENT_REQUIRED,
            Json(json!({"error":{"message":reason,"type":"budget_reached"}})),
        )
            .into_response());
    }
    let mut body = input.body;
    if !body.is_object() {
        return Err(anyhow::anyhow!("Invalid model request").into());
    }
    if body["stream"] == true {
        body["stream_options"] = json!({"include_usage": true});
    }
    // A catalog model goes to its routes in order; any other model to the assignment's
    // provider, as before the catalog (MG1).
    let catalog = crate::identity::models::catalog::Files::from(&state.app.config)
        .map(|files| files.load())
        .transpose()?;
    let entry = catalog.as_ref().and_then(|c| c.find(&model));
    // What people are charged for this model, and each route's cost to us (MG2).
    let price = entry.map(|e| e.price.clone()).unwrap_or_default();
    let routes: Vec<(String, String, Price)> = match entry {
        Some(entry) => {
            crate::identity::models::router::order(entry, crate::identity::models::router::roll())
                .into_iter()
                .map(|r| (r.provider, r.model, r.cost))
                .collect()
        }
        None => vec![(name, model.clone(), Price::default())],
    };
    let (job, person) = match &record.work {
        super::super::postgres::worker_control::Work::Reply(assignment) => (
            Some(assignment.job.reply.clone()),
            assignment.job.requester.clone(),
        ),
        super::super::postgres::worker_control::Work::Task(task) => (None, task.actor.clone()),
    };
    let mut tried = 0;
    let mut chosen = None;
    let failures = pg(&state).clone();
    let workspace = input.assignment.scope.workspace.clone();
    // Kept for the operator view (MG4); a failure to record never blocks the call.
    let failed = |provider: &str, status: u16| {
        crate::identity::models::router::failed(provider);
        let (store, w, model, provider) = (
            failures.clone(),
            workspace.clone(),
            model.clone(),
            provider.to_owned(),
        );
        async move {
            let _ = store
                .record_model_failure(&w, &provider, &model, status)
                .await;
        }
    };
    for (index, (name, provider_model, cost)) in routes.iter().enumerate() {
        tried += 1;
        let last = index + 1 == routes.len();
        let (base, key) = provider(&file, name)?;
        body["model"] = json!(provider_model);
        let sent = std::time::Instant::now();
        match upstream_client()
            .post(format!("{base}/chat/completions"))
            .bearer_auth(&key)
            .json(&body)
            .timeout(std::time::Duration::from_secs(600))
            .send()
            .await
        {
            // Another route is tried only before any of the answer has been sent on.
            Ok(response)
                if !last
                    && crate::identity::models::router::retryable(response.status().as_u16()) =>
            {
                failed(name, response.status().as_u16()).await;
            }
            Ok(response) => {
                let status = response.status().as_u16();
                if response.status().is_success() {
                    crate::identity::models::router::succeeded(name);
                } else if crate::identity::models::router::retryable(status) {
                    failed(name, status).await;
                } else {
                    // The request itself was refused: counted, but the provider isn't rested.
                    let _ = pg(&state)
                        .record_model_failure(&workspace, name, &model, status)
                        .await;
                }
                chosen = Some((
                    response,
                    name.clone(),
                    provider_model.clone(),
                    cost.clone(),
                    sent,
                ));
                break;
            }
            Err(_) => failed(name, 0).await,
        }
    }
    let Some((response, name, provider_model, cost, sent)) = chosen else {
        return Ok((
            StatusCode::BAD_GATEWAY,
            Json(json!({"error":{"message":"The model provider could not be reached"}})),
        )
            .into_response());
    };
    let status = response.status();
    let kind = response.headers().get(header::CONTENT_TYPE).cloned();
    // Forward as it arrives; keep the tail to read token counts when it ends.
    let (sender, receiver) =
        tokio::sync::mpsc::channel::<std::result::Result<Bytes, std::io::Error>>(32);
    let store = state.app.store.clone();
    let scope = input.assignment.scope.clone();
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
        if status.is_success() {
            let (input_tokens, output_tokens) = usage(&tail).unwrap_or((0, 0));
            let _ = store
                .record_model_usage(&crate::identity::postgres::ModelCall {
                    workspace: &scope.workspace,
                    assignment: &scope.assignment,
                    agent: &scope.agent,
                    job: job.as_deref(),
                    provider: &name,
                    model: &model,
                    provider_model: &provider_model,
                    attempts: tried,
                    input_tokens,
                    output_tokens,
                    person: Some(&person),
                    charge_micro: micro_dollars(input_tokens, output_tokens, &price),
                    cost_micro: micro_dollars(input_tokens, output_tokens, &cost),
                    latency_ms: i64::try_from(sent.elapsed().as_millis()).ok(),
                })
                .await;
        }
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
