use super::{
    Completion, Message, Model,
    stream::{Accumulator, SseDecoder},
};
use crate::tools::shell_definition;
use anyhow::{Context, Result, bail, ensure};
use futures_util::StreamExt;
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct CompatibleModel {
    client: reqwest::Client,
    endpoint: String,
    api_key: Option<String>,
    model: String,
}

impl CompatibleModel {
    pub fn new(base_url: &str, api_key: Option<String>, model: String) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            endpoint: format!("{}/chat/completions", base_url.trim_end_matches('/')),
            api_key,
            model,
        })
    }

    async fn request(&self, messages: &[Message], deltas: mpsc::Sender<String>) -> Result<Message> {
        let key = self
            .api_key
            .as_ref()
            .context("Set FIREWORKS_API_KEY in your local .env, then restart Ayati")?;
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(key)
            .json(
                &json!({ "model": self.model, "messages": messages, "stream": true,
                "max_tokens": 8192, "tools": [shell_definition()], "tool_choice": "auto",
                "parallel_tool_calls": false }),
            )
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Could not connect to the model provider"))?;
        // Do not expose provider bodies: they may echo credentials or private request contents.
        ensure!(
            response.status().is_success(),
            "Model provider returned HTTP {}",
            response.status().as_u16()
        );
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::default();
        let mut accumulated = Accumulator::default();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| anyhow::anyhow!("Model connection interrupted"))?;
            for data in decoder.feed(&chunk)? {
                if data == "[DONE]" {
                    return accumulated.finish();
                }
                if let Some(text) = accumulated.push(&data)? {
                    let _ = deltas.send(text).await;
                }
            }
        }
        accumulated.finish()
    }
}

impl Model for CompatibleModel {
    fn complete<'a>(
        &'a self,
        messages: &'a [Message],
        deltas: mpsc::Sender<String>,
        cancel: CancellationToken,
    ) -> Completion<'a> {
        Box::pin(async move {
            tokio::select! {
                _ = cancel.cancelled() => bail!("Stopped"),
                result = tokio::time::timeout(Duration::from_secs(180), self.request(messages, deltas)) => {
                    result.context("Model request timed out")?
                }
            }
        })
    }
}
