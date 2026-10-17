use super::{Platform, identifier};
use anyhow::{Context, Result, ensure};
use rusqlite::params;
use serde_json::{Value, json};

impl Platform {
    pub async fn consumer_configs(&self, query: &str) -> Result<Value> {
        let data = self.client()?.consumer_search(query).await?;
        let rows = data["toolkit_connection_statuses"]
            .as_array()
            .context("MCP omitted toolkit discovery")?;
        let configs = rows
            .iter()
            .take(30)
            .filter_map(|r| r["toolkit"].as_str())
            .map(|toolkit| json!({"id":"consumer_mcp","name":"Composio Connect","toolkit":toolkit}))
            .collect::<Vec<_>>();
        Ok(json!({"mode":"consumer_mcp","configs":configs}))
    }
    pub async fn link_consumer(
        &self,
        toolkit: &str,
        label: &str,
        account: Option<&str>,
    ) -> Result<Value> {
        identifier(toolkit)?;
        ensure!(
            !label.trim().is_empty() && label.len() <= 100 && !label.chars().any(char::is_control),
            "Account label must contain 1–100 bytes without control characters"
        );
        let _gate = self.gate.write().await;
        let client = self.client()?;
        let accounts = client.consumer_accounts(toolkit).await?;
        let id = uuid::Uuid::new_v4().to_string();
        if let Some(account) = account {
            identifier(account)?;
            ensure!(
                accounts
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a["id"] == account && a["status"] == "ACTIVE"),
                "Select an active account belonging to this Composio Connect key"
            );
            self.database()?.execute("INSERT INTO connections(id,toolkit,label,account_id,auth_config_id,status) VALUES(?1,?2,?3,?4,'consumer_mcp','ACTIVE')",params![id,toolkit,label,account])?;
            return Ok(json!({"id":id,"connected":true}));
        }
        self.database()?.execute("INSERT INTO connections(id,toolkit,label,auth_config_id,status) VALUES(?1,?2,?3,'consumer_mcp','LINK_UNCERTAIN')",params![id,toolkit,label])?;
        let value = client
            .mcp
            .as_ref()
            .context("Consumer MCP unavailable")?
            .call(
                "COMPOSIO_MANAGE_CONNECTIONS",
                json!({"toolkits":[{"name":toolkit,"action":"add","alias":label}]}),
            )
            .await?;
        let result = &value["data"]["results"][toolkit];
        let account = result["connected_account_id"]
            .as_str()
            .or_else(|| result["account_id"].as_str())
            .context("MCP did not acknowledge an account ID; inspect Composio before retrying")?;
        identifier(account)?;
        let url = result["redirect_url"]
            .as_str()
            .context("MCP omitted the account sign-in URL")?;
        let parsed = reqwest::Url::parse(url)?;
        ensure!(
            parsed.scheme() == "https"
                && parsed.username().is_empty()
                && parsed.password().is_none(),
            "Invalid MCP sign-in URL"
        );
        self.database()?.execute(
            "UPDATE connections SET account_id=?2,status='INITIALIZING' WHERE id=?1",
            params![id, account],
        )?;
        Ok(json!({"id":id,"redirect_url":url}))
    }
    pub async fn discover_connector_tools(&self, id: &str, query: &str) -> Result<Value> {
        ensure!(query.len() <= 100, "Search is too long");
        let (_, toolkit, status) = self.account_for(id)?;
        ensure!(
            status == "ACTIVE",
            "Refresh an active account before discovering operations"
        );
        let data = self.client()?.search_tools(&toolkit, query).await?;
        if self.client()?.is_consumer() {
            let mut db = self.database()?;
            let tx = db.transaction()?;
            for item in data["items"]
                .as_array()
                .context("Invalid operation catalog")?
            {
                let tool = item["slug"].as_str().context("Missing tool slug")?;
                let version = item["version"]
                    .as_str()
                    .context("Missing schema revision")?;
                tx.execute("INSERT INTO consumer_tools VALUES(?1,?2,?3) ON CONFLICT(connection_id,tool) DO UPDATE SET version=excluded.version",params![id,tool,version])?;
            }
            tx.commit()?;
        }
        Ok(data)
    }
}
