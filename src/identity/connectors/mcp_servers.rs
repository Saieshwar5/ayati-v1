//! Remote MCP servers an operator configured in a private file, with their
//! reviewed operations and bearer files. Accounts are verified at once (no
//! sign-in page), and the operator holds the credentials, so disconnecting
//! here only stops Cuckoo from using them.
use super::{Account, App, Call, Connect, ConnectorProvider, Outcome, Refusal, Saved};
use crate::identity::control::{
    catalog::{self, Server},
    mcp::Remote,
    model::Operation,
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

pub(super) struct McpServers {
    file: Option<PathBuf>,
}
impl McpServers {
    pub(super) fn new(file: Option<PathBuf>) -> Self {
        Self { file }
    }
    fn server(&self, workspace: &str, id: &str) -> Result<Server> {
        catalog::servers_in(self.file.as_deref(), workspace)?
            .into_iter()
            .find(|s| s.id == id)
            .context("Remote MCP profile is unavailable for this workspace")
    }
}

#[async_trait::async_trait]
impl ConnectorProvider for McpServers {
    fn name(&self) -> &'static str {
        "mcp"
    }
    fn apps(&self, workspace: &str) -> Result<Vec<App>> {
        Ok(catalog::servers_in(self.file.as_deref(), workspace)?
            .into_iter()
            .map(|s| App {
                id: s.id,
                name: s.name,
                account: s.account,
                operations: s.operations,
                provider: "mcp",
            })
            .collect())
    }
    async fn connect(&self, workspace: &str, app: &str, _callback: &str) -> Result<Connect> {
        let server = self.server(workspace, app)?;
        let fingerprint = server.fingerprint()?;
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut remote = Remote::connect(server.clone(), &fingerprint).await?;
            remote.verify(&server.operations).await
        })
        .await
        .context("Connection verification timed out")?
        .context(
            "The configured account could not be verified. Check its connection and try again.",
        )?;
        ensure!(
            self.server(workspace, app)?.fingerprint()? == fingerprint,
            "Connection changed during verification"
        );
        Ok(Connect::Ready(Account {
            definition: server.public(),
            binding: None,
            fingerprint,
        }))
    }
    async fn finish(&self, _: &str, _: &str, _: &Value) -> Result<Account> {
        anyhow::bail!("This connector does not use a sign-in page")
    }
    async fn disconnect(&self, _: &str, _: &str, _: Option<&Value>) -> Result<()> {
        Ok(())
    }
    async fn prepare(&self, saved: &Saved<'_>, operation: &Operation) -> Result<Box<dyn Call>> {
        let server = self.server(saved.workspace, saved.server)?;
        let current = server.fingerprint()?;
        if current != saved.fingerprint
            || serde_json::to_value(server.operation(&operation.id)?)?
                != serde_json::to_value(operation)?
        {
            return Err(Refusal::Changed.into());
        }
        let mut remote = Remote::connect(server, saved.fingerprint).await?;
        remote.verify(std::slice::from_ref(operation)).await?;
        ensure!(
            self.server(saved.workspace, saved.server)?.fingerprint()? == current,
            "Connection changed during setup"
        );
        Ok(Box::new(McpCall {
            remote,
            operation: operation.clone(),
        }))
    }
}

struct McpCall {
    remote: Remote,
    operation: Operation,
}
#[async_trait::async_trait]
impl Call for McpCall {
    async fn run(mut self: Box<Self>, arguments: &Value) -> Outcome {
        match self.remote.call(&self.operation, arguments).await {
            Ok(value) => Outcome::Completed(value),
            Err(_) => Outcome::Uncertain,
        }
    }
}
