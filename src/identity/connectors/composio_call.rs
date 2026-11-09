//! One checked Composio action, run once. Only a reply Composio marks
//! successful counts as done; anything else is unconfirmed and never retried.
use super::{
    Call, Outcome,
    composio_api::{ComposioApi, Reply},
    curated,
};
use reqwest::Method;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Results larger than this are summarized instead of returned to the agent.
const MAX_RESULT: usize = 64 * 1024;

pub(super) struct ComposioCall {
    pub api: ComposioApi,
    pub body: Value,
    pub tool: String,
    pub fields: BTreeMap<String, String>,
}
#[async_trait::async_trait]
impl Call for ComposioCall {
    async fn run(mut self: Box<Self>, arguments: &Value, _key: &str) -> Outcome {
        let Ok(arguments) = curated::provider_arguments(&self.fields, arguments) else {
            return Outcome::Uncertain;
        };
        self.body["arguments"] = arguments;
        let path = format!("tools/execute/{}", self.tool);
        match self.api.send(Method::POST, &path, Some(&self.body)).await {
            Ok(Reply::Ok(reply)) if reply["successful"] == true && reply["error"].is_null() => {
                let data = &reply["data"];
                Outcome::Completed(if data.to_string().len() <= MAX_RESULT {
                    json!({"data": data})
                } else {
                    json!({"message":"The result was too large to return. Ask for fewer items or a narrower search."})
                })
            }
            _ => Outcome::Uncertain,
        }
    }
}
