//! The Communication MCP adapter (`mcp`): a developer's MCP server bridges one
//! platform, such as Slack or SMS. Cuckoo calls the server's five tools; the
//! agent never sees the server, its tools or the platform's credentials.
//! Protocol v1 is described in `runtime/communication-gateway.md`.
use super::adapters::{Batch, Event, Outgoing, Route, Sent, Server, Source, TransportAdapter};
use super::model::Message;
use crate::identity::control::{catalog, mcp::Remote};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Events per read: up to 20 messages of 12,000 bytes fit the result limit.
const FETCH_LIMIT: usize = 20;
const RESULT_LIMIT: usize = 512 * 1024;
const REASON_CHARS: usize = 300;

/// One transport server from the operator's private file. It serves exactly
/// one workspace; its bearer token stays in its own private file.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    id: String,
    workspace: String,
    name: String,
    url: String,
    #[serde(default)]
    bearer_file: Option<String>,
}

pub(crate) struct McpTransport {
    file: Option<PathBuf>,
}
impl McpTransport {
    pub(crate) fn new(file: Option<PathBuf>) -> Self {
        Self { file }
    }
    /// Read on every use, so the operator can change servers and rotate tokens
    /// without a restart.
    fn profiles(&self) -> Result<Vec<Profile>> {
        let Some(file) = &self.file else {
            return Ok(vec![]);
        };
        let profiles: Vec<Profile> =
            serde_json::from_str(&crate::identity::private_config::read(file)?)
                .map_err(|_| anyhow::anyhow!("Invalid transport server configuration"))?;
        ensure!(profiles.len() <= 50, "Too many transport servers");
        let mut seen = std::collections::HashSet::new();
        for p in &profiles {
            ensure!(
                seen.insert((&p.workspace, &p.id)),
                "Duplicate transport server"
            );
            crate::identity::identifier(&p.id)?;
            ensure!(
                !p.name.trim().is_empty() && p.name.len() <= 80,
                "Invalid transport server name"
            );
            crate::identity::control::endpoint(&p.url)?;
        }
        Ok(profiles)
    }
    async fn connect(&self, workspace: &str, server: &str) -> Result<Remote> {
        let p = self
            .profiles()?
            .into_iter()
            .find(|p| p.workspace == workspace && p.id == server)
            .context("This transport server is not configured")?;
        let server = catalog::Server {
            id: p.id,
            workspace: p.workspace,
            name: p.name,
            account: None,
            url: p.url,
            bearer_file: p.bearer_file,
            operations: vec![],
        };
        let fingerprint = server.fingerprint()?;
        let remote = Remote::connect(server, &fingerprint)
            .await?
            .limit(RESULT_LIMIT);
        ensure!(
            remote.capabilities()["experimental"]["cuckoo/transport"]["version"] == 1,
            "The server does not speak the Cuckoo transport protocol, version 1"
        );
        Ok(remote)
    }
}

/// A tool's structured result; an error result is an error.
async fn tool(remote: &mut Remote, name: &str, arguments: Value) -> Result<Value> {
    let result = remote.call_tool(name, &arguments).await?;
    ensure!(
        result["isError"] != true,
        "The transport server refused {name}"
    );
    result
        .get("structuredContent")
        .filter(|v| v.is_object())
        .cloned()
        .context("The transport server returned no structured result")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoints {
    endpoints: Vec<Listed>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Listed {
    address: String,
    #[serde(default)]
    #[allow(dead_code)]
    name: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inbound {
    events: Vec<Value>,
    next: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Incoming {
    id: String,
    endpoint: String,
    sender: Sender,
    conversation: Conversation,
    text: String,
    #[serde(default)]
    automated: bool,
    #[serde(default)]
    #[allow(dead_code)]
    at: Option<String>,
}
/// The sender's address on the platform. A display name is accepted but never
/// used: who a sender is comes from Cuckoo's bindings, not from the platform.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sender {
    address: String,
    #[serde(default)]
    #[allow(dead_code)]
    name: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Conversation {
    id: String,
    kind: String,
    #[serde(default)]
    thread: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptEvent {
    #[allow(dead_code)]
    id: String,
    key: String,
    state: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Outcome {
    state: String,
    #[serde(default)]
    provider_id: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 320 && !value.chars().any(char::is_control)
}
/// Platform words shown to admins only: bounded, one line.
fn reason(value: Option<&str>, otherwise: &str) -> String {
    let cleaned: String = value
        .unwrap_or(otherwise)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(REASON_CHARS)
        .collect();
    if cleaned.trim().is_empty() {
        otherwise.into()
    } else {
        cleaned
    }
}

/// One event, or `None` when it is malformed or unsupported. Version 1 accepts
/// direct conversations only: in a group, being in the group is no permission.
fn event(value: Value) -> Option<Event> {
    let mut value = value;
    let kind = value.as_object_mut()?.remove("type")?;
    match kind.as_str()? {
        "message" => {
            let m: Incoming = serde_json::from_value(value).ok()?;
            if m.conversation.kind != "direct" {
                return None;
            }
            let message = Message {
                event: m.id,
                sender: m.sender.address,
                recipient: m.endpoint,
                conversation: m.conversation.id,
                text: m.text,
                attachments: vec![],
                automated: m.automated,
                thread: m.conversation.thread,
            };
            message.validate().ok()?;
            Some(Event::Message(message))
        }
        "receipt" => {
            let r: ReceiptEvent = serde_json::from_value(value).ok()?;
            if !text(&r.key) {
                return None;
            }
            match r.state.as_str() {
                "delivered" | "read" => Some(Event::Receipt {
                    key: r.key,
                    delivered: true,
                }),
                "failed" => Some(Event::Receipt {
                    key: r.key,
                    delivered: false,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `send_message` and `message_status` answers. Anything unclear is uncertain.
fn outcome(value: &Value) -> Sent {
    let unclear = || Sent::Uncertain("The transport server's answer was unclear".into());
    let Ok(o) = serde_json::from_value::<Outcome>(value.clone()) else {
        return unclear();
    };
    if o.provider_id
        .as_deref()
        .is_some_and(|p| !text(p) || p.len() > 256)
    {
        return unclear();
    }
    match o.state.as_str() {
        "accepted" => Sent::Accepted(o.provider_id),
        "delivered" => Sent::Delivered(o.provider_id),
        "refused" => Sent::Refused(reason(
            o.reason.as_deref(),
            "The platform refused the message",
        )),
        "uncertain" | "unknown" => Sent::Uncertain(reason(
            o.reason.as_deref(),
            "The platform cannot confirm the message yet",
        )),
        _ => unclear(),
    }
}

#[async_trait::async_trait]
impl TransportAdapter for McpTransport {
    fn name(&self) -> &'static str {
        "mcp"
    }
    fn servers(&self, workspace: &str) -> Result<Vec<Server>> {
        Ok(self
            .profiles()?
            .into_iter()
            .filter(|p| p.workspace == workspace)
            .map(|p| Server {
                id: p.id,
                name: p.name,
            })
            .collect())
    }
    async fn check(&self, route: &Route) -> Result<()> {
        let mut remote = self.connect(&route.workspace, &route.server).await?;
        let info = tool(&mut remote, "transport_info", json!({})).await?;
        ensure!(
            info["name"].as_str().is_some_and(text),
            "The transport server did not describe itself"
        );
        let listed: Endpoints =
            serde_json::from_value(tool(&mut remote, "list_endpoints", json!({})).await?)
                .context("The transport server listed its addresses incorrectly")?;
        ensure!(
            listed.endpoints.len() <= 100,
            "Too many transport addresses"
        );
        ensure!(
            listed.endpoints.iter().any(|e| e.address == route.address),
            "The transport server does not serve this address"
        );
        Ok(())
    }
    async fn fetch(&self, source: &Source, cursor: Option<&str>) -> Result<Batch> {
        let mut remote = self.connect(&source.workspace, &source.server).await?;
        let read = tool(
            &mut remote,
            "fetch_inbound",
            json!({"after":cursor,"limit":FETCH_LIMIT}),
        )
        .await?;
        let read: Inbound =
            serde_json::from_value(read).context("The transport server's events are malformed")?;
        ensure!(
            read.events.len() <= FETCH_LIMIT,
            "The transport server sent too many events"
        );
        ensure!(
            read.next
                .as_deref()
                .is_none_or(|n| text(n) && n.len() <= 256),
            "The transport server's cursor is invalid"
        );
        let total = read.events.len();
        let events: Vec<Event> = read.events.into_iter().filter_map(event).collect();
        Ok(Batch {
            skipped: total - events.len(),
            events,
            next: read.next,
        })
    }
    async fn send(&self, route: &Route, out: &Outgoing) -> Sent {
        let Ok(mut remote) = self.connect(&route.workspace, &route.server).await else {
            return Sent::Unreached("The transport server could not be reached".into());
        };
        let arguments = json!({"key":out.key,"endpoint":route.address,"to":out.to,
            "conversation":out.conversation,"thread":out.thread,"text":out.text});
        let Ok(result) = remote.call_tool("send_message", &arguments).await else {
            return Sent::Uncertain("The transport server did not confirm the send".into());
        };
        if result["isError"] == true {
            let said = result["content"][0]["text"].as_str();
            return Sent::Refused(reason(said, "The transport server refused the message"));
        }
        outcome(&result["structuredContent"])
    }
    async fn status(&self, route: &Route, key: &str) -> Sent {
        let unreachable = || Sent::Uncertain("The transport server could not be asked".into());
        let Ok(mut remote) = self.connect(&route.workspace, &route.server).await else {
            return unreachable();
        };
        match tool(&mut remote, "message_status", json!({"key":key})).await {
            Ok(answer) => outcome(&answer),
            Err(_) => unreachable(),
        }
    }
}
