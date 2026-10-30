//! Reviewable setup data never carries a credential or execution capability.
use super::model::{Operation, Rule};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Start {
    pub id: String,
    pub purpose: String,
    pub chat: String,
    pub connections: Vec<String>,
    pub assistant: bool,
}
impl Start {
    pub fn validate(&self) -> Result<()> {
        super::super::worker_control::model::uuid(&self.id)?;
        ensure!(
            !self.purpose.trim().is_empty() && self.purpose.len() <= 4000,
            "Describe the agent's responsibility in under 4000 bytes"
        );
        ensure!(
            !self.connections.is_empty() && self.connections.len() <= 5,
            "Review one to five accounts at a time"
        );
        let unique: std::collections::HashSet<_> = self.connections.iter().collect();
        ensure!(
            unique.len() == self.connections.len(),
            "Duplicate account selection"
        );
        Ok(())
    }
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Permission {
    #[serde(default)]
    pub approvers: Vec<Value>,
    pub operation: Operation,
    pub ceiling: String,
    pub approver: Option<String>,
    pub approver_name: Option<String>,
    pub delegation_revision: Option<i64>,
    pub rule_revision: i64,
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Account {
    pub id: String,
    pub server: String,
    pub name: String,
    pub account: Option<String>,
    pub revision: i64,
    pub fingerprint: String,
    pub permissions: Vec<Permission>,
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Context {
    #[serde(default)]
    pub approval_channels: Vec<Value>,
    pub agent_revision: i64,
    pub name: String,
    pub responsibility: String,
    pub model: String,
    pub provider: Option<String>,
    pub accounts: Vec<Account>,
    pub conversations: Vec<Value>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Choice {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approver_revision: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_chat: Option<String>,
    pub connection: String,
    pub operation: String,
    pub mode: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Inventory {
    pub connection: String,
    pub tools: Vec<String>,
    pub unsupported: Vec<String>,
}
/// Progress of optional AI suggestions, which an executor prepares after the
/// draft is saved with every action blocked.
#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub(crate) struct Suggestion {
    /// `pending`, `ready`, `failed` or `discarded` (the draft was edited first).
    pub state: String,
    pub detail: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Draft {
    pub request: Start,
    pub context: Context,
    pub inventories: Vec<Inventory>,
    pub choices: Vec<Choice>,
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<Suggestion>,
}
impl Draft {
    pub fn rules(&self) -> Result<Vec<Rule>> {
        let mut rules = Vec::new();
        for account in &self.context.accounts {
            for p in &account.permissions {
                let matches: Vec<_> = self
                    .choices
                    .iter()
                    .filter(|c| c.connection == account.id && c.operation == p.operation.id)
                    .collect();
                ensure!(
                    matches.len() == 1,
                    "Every reviewed action needs exactly one choice"
                );
                let mode = &matches[0].mode;
                ensure!(
                    ["allow", "ask", "block"].contains(&mode.as_str()),
                    "Unsupported permission"
                );
                ensure!(
                    mode == "block"
                        || p.ceiling == "allow"
                        || (mode == "ask" && p.ceiling == "ask"),
                    "A choice exceeds the admin's delegated limit"
                );
                rules.push(Rule {
                    approver: if mode == "ask" {
                        matches[0].approver.clone()
                    } else {
                        None
                    },
                    approver_revision: if mode == "ask" {
                        matches[0].approver_revision
                    } else {
                        None
                    },
                    approval_chat: if mode == "ask" {
                        matches[0].approval_chat.clone()
                    } else {
                        None
                    },
                    connection: account.id.clone(),
                    operation: p.operation.id.clone(),
                    mode: mode.clone(),
                    revision: p.rule_revision,
                    chat: self.request.chat.clone(),
                    delegation_revision: p.delegation_revision,
                });
            }
        }
        ensure!(
            rules.len() == self.choices.len(),
            "Unknown or duplicate action choice"
        );
        Ok(rules)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Edit {
    pub revision: i64,
    pub choices: Vec<Choice>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Preview {
    pub revision: i64,
    pub example: Option<super::model::ActionRequest>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Activate {
    pub revision: i64,
    pub reviewed: bool,
}
