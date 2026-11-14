//! Connected systems (C1 of the connected systems plan, D249): a company's own
//! MCP server, connected by an agent's owner or an admin from the agent's Tools
//! tab, with no operator file. The connection is an ordinary workspace
//! connection with provider `system`, so its tools pass the same gateway, rules,
//! approvals, call records and rate limit as every other app.
//!
//! The server's address and its sealed key are the connection's binding. The
//! key is sealed with `AYATI_SECRETS_KEY_FILE`, opened only to make a call, and
//! never reaches the model, logs, Activity or the browser after it is entered.
use super::system_tools::{self, Reviewed};
use super::{
    Account, App, Call, Capabilities, Connect, ConnectorProvider, Outcome, Refusal, Saved,
};
use crate::identity::control::{catalog::Server, mcp::Remote, model::Operation};
use crate::identity::secrets::Sealer;
use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

static SECRETS: OnceLock<PathBuf> = OnceLock::new();

/// Set once at controller start: the file that seals systems' keys.
pub(crate) fn configure(secrets_key_file: Option<PathBuf>) {
    if let Some(path) = secrets_key_file {
        let _ = SECRETS.set(path);
    }
}

fn sealer() -> Result<Sealer> {
    SECRETS.get().map(|p| Sealer::new(p)).context(
        "Connecting a system with a key needs AYATI_SECRETS_KEY_FILE on this server. Ask your operator.",
    )
}

/// The label a system's key is sealed under: a sealed key moved to another
/// connection can't be opened.
fn label(workspace: &str, server: &str) -> String {
    format!("connected-system:{workspace}:{server}")
}

/// The connection's server name for a connection id; unique per workspace.
pub(crate) fn server_name(connection: &str) -> String {
    format!("system-{connection}")
}

/// The circuit breaker a system's calls count against: its own, so one broken
/// system never pauses another.
pub(crate) fn breaker(server: &str) -> Option<String> {
    let id = server.strip_prefix("system-")?;
    let compact: String = id
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(28)
        .collect();
    (compact.len() == 28).then(|| format!("sys_{}", compact.to_ascii_lowercase()))
}

/// What the person typed: the address and, if the server wants one, its key.
pub(crate) struct Address {
    pub url: String,
    pub key: Option<String>,
}
impl Address {
    pub(crate) fn validate(&self) -> Result<()> {
        crate::identity::control::endpoint(&self.url)?;
        if let Some(key) = &self.key {
            ensure!(
                !key.trim().is_empty() && key.len() <= 4096 && !key.chars().any(char::is_control),
                "Use a key of up to 4,096 characters with no line breaks"
            );
        }
        Ok(())
    }
    /// The binding saved with the connection: the address and the sealed key.
    pub(crate) fn binding(&self, workspace: &str, server: &str) -> Result<Value> {
        let key = match &self.key {
            Some(key) => Some(
                base64::engine::general_purpose::STANDARD
                    .encode(sealer()?.seal(&label(workspace, server), key.trim())?),
            ),
            None => None,
        };
        Ok(json!({"url":self.url,"key":key}))
    }
}

fn server(workspace: &str, name: &str, url: &str, operations: Vec<Operation>) -> Server {
    Server {
        id: name.to_owned(),
        workspace: workspace.to_owned(),
        name: name.to_owned(),
        account: None,
        url: url.to_owned(),
        bearer_file: None,
        operations,
    }
}

/// Connects to the system and reviews every tool it lists. Nothing is saved.
pub(crate) async fn list(workspace: &str, address: &Address) -> Result<Vec<Reviewed>> {
    address.validate()?;
    let tools = tokio::time::timeout(Duration::from_secs(30), async {
        let mut remote = Remote::connect_with(
            server(workspace, "system-preview", &address.url, vec![]),
            address.key.as_ref().map(|k| k.trim().to_owned()),
        )
        .await
        .map_err(plain)?;
        remote.tools().await
    })
    .await
    .context("The system didn't answer within 30 seconds. Check the address and try again.")??;
    ensure!(
        tools.len() <= system_tools::MAX_LISTED,
        "The system lists more than 200 tools"
    );
    Ok(tools.iter().map(system_tools::review).collect())
}

/// The tools of a saved system, listed with its saved key, for a new review.
pub(crate) async fn list_saved(
    workspace: &str,
    server_name: &str,
    binding: &Value,
) -> Result<Vec<Reviewed>> {
    let saved = Saved {
        workspace,
        server: server_name,
        binding: Some(binding),
        fingerprint: "",
    };
    let (url, key) = opened(&saved)?;
    list(workspace, &Address { url, key }).await
}

/// What went wrong reaching a system, said so the person knows what to fix.
fn plain(error: anyhow::Error) -> anyhow::Error {
    let text = error.to_string();
    anyhow::anyhow!(if text.contains("restricted address") {
        "That address is on a private network. Cuckoo can reach public addresses only.".to_owned()
    } else if text.contains("rejected the request") {
        "The system refused the request. Check the key.".to_owned()
    } else if text.contains("did not acknowledge") || text.contains("timed out") {
        "Cuckoo can't reach that address. Check it and try again.".to_owned()
    } else if text.contains("MCP version")
        || text.contains("no MCP tools")
        || text.contains("matching result")
    {
        "That address doesn't answer as an MCP server.".to_owned()
    } else {
        text
    })
}

/// The address and opened key from a saved connection's binding.
fn opened(saved: &Saved<'_>) -> Result<(String, Option<String>)> {
    let binding = saved.binding.context("The system's address is missing")?;
    let url = binding["url"]
        .as_str()
        .context("The system's address is missing")?;
    let key = match binding["key"].as_str() {
        Some(sealed) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(sealed)
                .context("The system's key is damaged")?;
            Some(sealer()?.open(&label(saved.workspace, saved.server), &bytes)?)
        }
        None => None,
    };
    Ok((url.to_owned(), key))
}

/// The tool as the system lists it now, reviewed; `None` when it's gone.
async fn current(remote: &mut Remote, tool: &str) -> Result<Option<Reviewed>> {
    Ok(remote
        .tools()
        .await?
        .iter()
        .find(|t| t["name"] == tool)
        .map(system_tools::review))
}

pub(super) struct Systems;

#[async_trait::async_trait]
impl ConnectorProvider for Systems {
    fn name(&self) -> &'static str {
        "system"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    /// Systems are connected from an agent's Tools tab, never offered in Add app.
    fn apps(&self, _workspace: &str) -> Result<Vec<App>> {
        Ok(vec![])
    }
    async fn connect(&self, _: &str, _: &str, _: &str) -> Result<Connect> {
        anyhow::bail!("Connect a system from the agent's Tools tab")
    }
    async fn finish(&self, _: &str, _: &str, _: &Value) -> Result<Account> {
        anyhow::bail!("Connected systems don't use a sign-in page")
    }
    async fn disconnect(&self, _: &str, _: &str, _: Option<&Value>) -> Result<()> {
        Ok(())
    }
    /// The tool must still look as it was reviewed; a changed or missing tool is
    /// held (refused) until someone reviews it again, while the others work.
    async fn prepare(&self, saved: &Saved<'_>, operation: &Operation) -> Result<Box<dyn Call>> {
        let (url, key) = opened(saved)?;
        let mut remote = Remote::connect_with(
            server(saved.workspace, saved.server, &url, vec![operation.clone()]),
            key,
        )
        .await?;
        let now = current(&mut remote, &operation.tool).await?;
        if now.and_then(|t| t.schema) != Some(operation.schema.clone()) {
            return Err(Refusal::Changed.into());
        }
        Ok(Box::new(SystemCall {
            remote,
            operation: operation.clone(),
        }))
    }
    /// For the nightly drift check: the tool's input as Cuckoo reviews it now.
    async fn describe(&self, saved: &Saved<'_>, operation: &Operation) -> Result<Value> {
        let (url, key) = opened(saved)?;
        let mut remote =
            Remote::connect_with(server(saved.workspace, saved.server, &url, vec![]), key).await?;
        let schema = current(&mut remote, &operation.tool)
            .await?
            .map(|t| t.schema.unwrap_or_else(|| json!({"unsupported": t.reason})));
        Ok(json!({"version":null,"schema":schema,"latest":null}))
    }
}

struct SystemCall {
    remote: Remote,
    operation: Operation,
}
#[async_trait::async_trait]
impl Call for SystemCall {
    async fn run(mut self: Box<Self>, arguments: &Value, _key: &str) -> Outcome {
        match self.remote.call(&self.operation, arguments).await {
            Ok(value) => Outcome::Completed(value),
            Err(_) => Outcome::Uncertain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_system_has_its_own_breaker_name() {
        let a = breaker(&server_name("6f1c2b4e-0d1a-4c5b-9e7f-1234567890ab")).unwrap();
        let b = breaker(&server_name("7a1c2b4e-0d1a-4c5b-9e7f-1234567890ab")).unwrap();
        assert_ne!(a, b);
        assert!(a.len() <= 32 && a.starts_with("sys_"));
        assert!(
            a.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        );
        assert_eq!(breaker("gmail"), None);
    }
    #[test]
    fn addresses_must_be_public_https_and_keys_one_line() {
        let ok = Address {
            url: "https://studyhub.example/mcp".into(),
            key: Some("secret-key".into()),
        };
        ok.validate().unwrap();
        for url in [
            "http://studyhub.example/mcp",
            "https://studyhub.example/mcp?key=1",
        ] {
            assert!(
                Address {
                    url: url.into(),
                    key: None
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            Address {
                url: "https://studyhub.example/mcp".into(),
                key: Some("two\nlines".into())
            }
            .validate()
            .is_err()
        );
    }
}
