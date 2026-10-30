//! Chat request types and queue errors shared by the HTTP layer and PostgreSQL.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewChat {
    pub id: String,
    pub name: String,
    pub users: Vec<String>,
    pub agents: Vec<String>,
    #[serde(default = "group_kind")]
    pub kind: String,
    #[serde(default)]
    pub description: String,
}
fn group_kind() -> String {
    "group".into()
}
impl NewChat {
    pub fn direct_key(&self, actor: &str) -> Result<Option<String>> {
        ensure!(
            ["group", "channel", "direct"].contains(&self.kind.as_str()),
            "Invalid conversation type"
        );
        ensure!(self.description.len() <= 300, "Purpose is too long");
        let others: std::collections::BTreeSet<_> = self
            .users
            .iter()
            .filter(|id| id.as_str() != actor)
            .collect();
        let agents: std::collections::BTreeSet<_> = self.agents.iter().collect();
        if self.kind != "group" {
            ensure!(
                others.len() + agents.len() >= 1,
                "Add at least one other person or agent"
            );
        }
        if self.kind != "direct" {
            return Ok(None);
        }
        ensure!(
            others.len() + agents.len() == 1,
            "Direct messages require exactly two participants"
        );
        let mut participants = vec![format!("user:{actor}")];
        participants.extend(others.iter().map(|id| format!("user:{id}")));
        participants.extend(agents.iter().map(|id| format!("agent:{id}")));
        participants.sort();
        Ok(Some(serde_json::to_string(&participants)?))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewMessage {
    #[serde(default)]
    pub attachments: Vec<String>,
    pub id: String,
    pub content: String,
    pub agent: Option<String>,
    #[serde(default)]
    pub thread: Option<String>,
    /// An earlier message of the same conversation (and thread) this one quotes.
    #[serde(default)]
    pub reply_to: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub reply: String,
    pub chat: String,
    pub workspace: String,
    pub requester: String,
    pub agent: String,
    pub prompt: String,
    #[serde(skip)]
    pub claim: Option<String>,
}
#[derive(Debug)]
pub struct QueueFull;
impl std::fmt::Display for QueueFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The workspace queue is full (64 pending replies). Wait for work to finish before retrying."
        )
    }
}
impl std::error::Error for QueueFull {}

#[derive(Debug)]
pub struct BackgroundQueueFull;
impl std::fmt::Display for BackgroundQueueFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The background queue is full (32 pending requests). Wait for work to finish before retrying."
        )
    }
}
impl std::error::Error for BackgroundQueueFull {}
