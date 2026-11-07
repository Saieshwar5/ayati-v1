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
    /// The one agent an older client asks; current clients send `agents`.
    #[serde(default)]
    pub agent: Option<String>,
    /// The agents this message asks; each answers separately (schema 99).
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub thread: Option<String>,
    /// An earlier message of the same conversation (and thread) this one quotes.
    #[serde(default)]
    pub reply_to: Option<String>,
}
/// The most agents one message may ask (the owner's decision of 2026-11-07).
pub const MAX_AGENTS: usize = 3;
impl NewMessage {
    /// Every agent this message asks, in the order given: `agents`, or the
    /// older single `agent`. At most [`MAX_AGENTS`], each once.
    pub fn asked(&self) -> Result<Vec<String>> {
        ensure!(
            self.agent.is_none() || self.agents.is_empty(),
            "Send agent or agents, not both"
        );
        let asked: Vec<String> = self.agent.iter().chain(&self.agents).cloned().collect();
        ensure!(
            asked.len() <= MAX_AGENTS,
            "Ask at most 3 agents in one message. Send another message for the others."
        );
        let distinct: std::collections::BTreeSet<_> = asked.iter().collect();
        ensure!(
            distinct.len() == asked.len(),
            "Ask each agent once per message"
        );
        Ok(asked)
    }
}
/// Why a message asking several agents was refused: each agent that can't take
/// it, by name, and that nothing was sent, so nobody thinks one agent got it.
pub fn refused(asked: usize, refusals: &[(String, String)]) -> String {
    let mut text = String::new();
    for (name, reason) in refusals {
        let reason = reason.trim();
        let end = if reason.ends_with('.') { "" } else { "." };
        text.push_str(&format!("{name} can't take this message: {reason}{end} "));
    }
    if refusals.len() < asked {
        let names: Vec<String> = refusals
            .iter()
            .map(|(name, _)| format!("@{name}"))
            .collect();
        text.push_str(&format!(
            "Nothing was sent; remove {} to ask the others.",
            names.join(" and ")
        ));
    } else {
        text.push_str("Nothing was sent.");
    }
    text
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
/// An admin paused all agents in the workspace; no agent work starts until they
/// resume. Schedules skip and events go unhandled instead of being disabled.
#[derive(Debug)]
pub struct AgentsPaused;
impl std::fmt::Display for AgentsPaused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "All agents in this workspace are paused. An admin can resume them from Activity or Settings."
        )
    }
}
impl std::error::Error for AgentsPaused {}

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

#[cfg(test)]
mod tests {
    use super::*;
    fn sent(body: serde_json::Value) -> Result<Vec<String>> {
        serde_json::from_value::<NewMessage>(body)?.asked()
    }
    #[test]
    fn a_message_asks_up_to_three_distinct_agents_or_the_older_single_agent() {
        let id = "00000000-0000-4000-8000-000000000001";
        let base = serde_json::json!({"id": id, "content": "Stock?"});
        assert_eq!(sent(base.clone()).unwrap(), Vec::<String>::new());
        let mut older = base.clone();
        older["agent"] = "a1".into();
        assert_eq!(sent(older.clone()).unwrap(), ["a1"]);
        let mut several = base.clone();
        several["agents"] = serde_json::json!(["n1", "a1"]);
        assert_eq!(sent(several).unwrap(), ["n1", "a1"], "the order asked");
        let mut both = older;
        both["agents"] = serde_json::json!(["n1"]);
        assert!(sent(both).is_err());
        let mut twice = base.clone();
        twice["agents"] = serde_json::json!(["a1", "a1"]);
        assert!(sent(twice).is_err());
        let mut four = base;
        four["agents"] = serde_json::json!(["a1", "n1", "o1", "p1"]);
        let error = sent(four).unwrap_err().to_string();
        assert!(error.contains("at most 3 agents"), "{error}");
    }
    #[test]
    fn a_refusal_names_each_agent_and_says_nothing_was_sent() {
        let one = refused(
            2,
            &[("Atlas".into(), "Agent is not shared with you".into())],
        );
        assert_eq!(
            one,
            "Atlas can't take this message: Agent is not shared with you. Nothing was sent; remove @Atlas to ask the others."
        );
        let all = refused(
            2,
            &[
                ("Atlas".into(), "This agent is paused.".into()),
                ("Nova".into(), "Agent is not in this conversation".into()),
            ],
        );
        assert!(all.starts_with("Atlas can't take this message: This agent is paused. Nova"));
        assert!(all.ends_with("Nothing was sent."), "{all}");
    }
}
