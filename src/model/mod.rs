mod compatible;
mod stream;

use anyhow::Result;
pub use compatible::CompatibleModel;
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn text(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: vec![],
            tool_call_id: None,
        }
    }
    pub fn tool(id: &str, content: String) -> Self {
        Self {
            tool_call_id: Some(id.into()),
            ..Self::text("tool", content)
        }
    }
}

pub type Completion<'a> = Pin<Box<dyn Future<Output = Result<Message>> + Send + 'a>>;

// Provider implementations translate this small boundary to their own wire protocol.
pub trait Model: Send + Sync {
    fn complete<'a>(
        &'a self,
        messages: &'a [Message],
        deltas: mpsc::Sender<String>,
        cancel: CancellationToken,
    ) -> Completion<'a>;
}
