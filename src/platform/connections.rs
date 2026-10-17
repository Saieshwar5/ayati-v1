use super::{Platform, identifier};
use anyhow::{Context, Result, ensure};
use reqwest::Method;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize)]
pub struct Connection {
    pub id: String,
    pub toolkit: String,
    pub label: String,
    pub status: String,
    pub revision: i64,
}
#[derive(Serialize)]
pub struct Grant {
    pub connection_id: String,
    pub tool: String,
    pub version: String,
    pub definition: Value,
}

impl Platform {
    pub fn connections(&self) -> Result<Vec<Connection>> {
        let db = self.database()?;
        let mut query =
            db.prepare("SELECT id,toolkit,label,status,revision FROM connections ORDER BY rowid")?;
        Ok(query
            .query_map([], |r| {
                Ok(Connection {
                    id: r.get(0)?,
                    toolkit: r.get(1)?,
                    label: r.get(2)?,
                    status: r.get(3)?,
                    revision: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub async fn link(&self, toolkit: &str, auth_config: &str, label: &str) -> Result<Value> {
        ensure!(
            ["gmail", "googledrive", "googlecalendar"].contains(&toolkit),
            "Choose Gmail, Google Drive or Google Calendar"
        );
        identifier(auth_config)?;
        ensure!(
            !label.trim().is_empty() && label.len() <= 100 && !label.chars().any(char::is_control),
            "Account label must contain 1–100 bytes without control characters"
        );
        let _gate = self.gate.write().await;
        let client = self.client()?;
        let id = uuid::Uuid::new_v4().to_string();
        // Persist uncertainty before the non-idempotent provider request.
        self.database()?.execute("INSERT INTO connections(id,toolkit,label,auth_config_id,status) VALUES(?1,?2,?3,?4,'LINK_UNCERTAIN')",params![id,toolkit,label,auth_config])?;
        let linked = client
            .request(
                Method::POST,
                "connected_accounts/link",
                Some(&json!({"auth_config_id":auth_config,"user_id":self.user_id})),
            )
            .await?;
        let account = linked["connected_account_id"]
            .as_str()
            .context("Composio did not return an account ID; inspect pending connections")?;
        identifier(account)?;
        let url = linked["redirect_url"]
            .as_str()
            .context("Composio did not return a sign-in URL")?;
        let parsed = reqwest::Url::parse(url)?;
        ensure!(
            parsed.scheme() == "https"
                && parsed.username().is_empty()
                && parsed.password().is_none(),
            "Invalid provider sign-in URL"
        );
        self.database()?.execute(
            "UPDATE connections SET account_id=?2,status='INITIALIZING' WHERE id=?1",
            params![id, account],
        )?;
        // Never return the link token or the account credential state.
        Ok(json!({"id":id,"redirect_url":url}))
    }
    fn account_for(&self, id: &str) -> Result<(String, String, String)> {
        self.database()?
            .query_row(
                "SELECT account_id,toolkit,status FROM connections WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .context("Connection is unavailable or has no acknowledged account")
    }
    pub async fn refresh_connection(&self, id: &str) -> Result<()> {
        let _gate = self.gate.write().await;
        let (account, toolkit, status) = self.account_for(id)?;
        ensure!(
            status != "REVOKED",
            "Connection was revoked locally; connect again deliberately"
        );
        let status = self
            .client()?
            .account(&account, &self.user_id, &toolkit)
            .await?;
        self.database()?.execute(
            "UPDATE connections SET status=?2,revision=revision+1 WHERE id=?1",
            params![id, status],
        )?;
        Ok(())
    }
    pub async fn revoke_connection(&self, id: &str) -> Result<()> {
        let _gate = self.gate.write().await;
        // Local revocation is immediate even if Composio/provider is unreachable.
        let changed = self.database()?.execute(
            "UPDATE connections SET status='REVOKED',revision=revision+1 WHERE id=?1",
            [id],
        )?;
        ensure!(changed == 1, "Unknown connection");
        self.database()?
            .execute("DELETE FROM grants WHERE connection_id=?1", [id])?;
        Ok(())
    }
    pub fn grants(&self, agent: &str) -> Result<Vec<Grant>> {
        let db = self.database()?;
        let mut query=db.prepare("SELECT g.connection_id,g.tool,g.version,g.definition FROM grants g JOIN connections c ON c.id=g.connection_id WHERE g.agent_id=?1 AND c.status='ACTIVE' ORDER BY g.tool")?;
        let rows = query.query_map([agent], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, String>(3)?))
        })?;
        rows.map(|row| {
            let (connection_id, tool, version, definition) = row?;
            Ok(Grant {
                connection_id,
                tool,
                version,
                definition: serde_json::from_str(&definition)?,
            })
        })
        .collect()
    }
    pub async fn set_grant(
        &self,
        agent: &str,
        id: &str,
        tool: &str,
        version: &str,
        enabled: bool,
    ) -> Result<()> {
        identifier(tool)?;
        identifier(version)?;
        ensure!(
            !["latest", "LATEST"].contains(&version),
            "Pin an explicit toolkit version before granting an operation"
        );
        let _gate = self.gate.write().await;
        if !enabled {
            self.database()?.execute(
                "DELETE FROM grants WHERE agent_id=?1 AND connection_id=?2 AND tool=?3",
                params![agent, id, tool],
            )?;
            return Ok(());
        }
        let (account, toolkit, status) = self.account_for(id)?;
        ensure!(
            status == "ACTIVE",
            "Refresh an active connection before granting operations"
        );
        ensure!(
            self.client()?
                .account(&account, &self.user_id, &toolkit)
                .await?
                == "ACTIVE",
            "Account is not active"
        );
        let definition = self
            .client()?
            .request(
                Method::GET,
                &format!("tools/{tool}?version={version}"),
                None,
            )
            .await?;
        ensure!(
            definition["toolkit"]["slug"] == toolkit && definition["slug"] == tool,
            "Tool does not belong to this connection"
        );
        ensure!(
            definition["version"] == version,
            "Composio returned a different tool version; no grant was saved"
        );
        ensure!(
            definition["input_parameters"].is_object(),
            "Tool schema is unavailable"
        );
        let definition = json!({"slug":tool,"description":definition["description"],"input_parameters":definition["input_parameters"]});
        ensure!(
            definition.to_string().len() <= 12000,
            "This operation schema exceeds the current tool budget"
        );
        let count: i64 = self.database()?.query_row(
            "SELECT COUNT(*) FROM grants WHERE agent_id=?1",
            [agent],
            |r| r.get(0),
        )?;
        ensure!(
            count < 16
                || self
                    .grants(agent)?
                    .iter()
                    .any(|g| g.connection_id == id && g.tool == tool),
            "At most 16 operations can be assigned to one agent in this increment"
        );
        self.database()?.execute("INSERT INTO grants VALUES(?1,?2,?3,?4,?5) ON CONFLICT(agent_id,connection_id,tool) DO UPDATE SET version=excluded.version,definition=excluded.definition",params![agent,id,tool,version,definition.to_string()])?;
        Ok(())
    }
    pub async fn execute_connector(
        &self,
        run: super::ExecutionRun<'_>,
        id: &str,
        tool: &str,
        arguments: &Value,
    ) -> Result<Value> {
        let super::ExecutionRun {
            agent,
            store,
            claim,
            cancel,
        } = run;
        let task = &claim.task_id;
        ensure!(
            arguments.is_object() && arguments.to_string().len() <= 12000,
            "Tool arguments must be a bounded JSON object"
        );
        let _gate = self.gate.read().await;
        let grant = self
            .grants(agent)?
            .into_iter()
            .find(|g| g.connection_id == id && g.tool == tool)
            .context("This agent has no current grant for the requested operation")?;
        let (account, toolkit, status) = self.account_for(id)?;
        ensure!(
            status == "ACTIVE"
                && self
                    .client()?
                    .account(&account, &self.user_id, &toolkit)
                    .await?
                    == "ACTIVE",
            "Connected account is unavailable; reconnect through Settings"
        );
        ensure!(
            !cancel.is_cancelled() && store.claim_active(claim)?,
            "Work was stopped before external execution"
        );
        let execution = uuid::Uuid::new_v4().to_string();
        self.database()?.execute("INSERT INTO executions(id,agent_id,task_id,connection_id,tool,status) VALUES(?1,?2,?3,?4,?5,'running')",params![execution,agent,task,id,tool])?;
        let result = self
            .client()?
            .execute(&account, &self.user_id, tool, &grant.version, arguments)
            .await;
        let (status, log) = match &result {
            Ok(value)
                if value["successful"] == true
                    && value.get("error").is_none_or(|v| v.is_null() || v == "") =>
            {
                ("completed", value["log_id"].as_str().map(str::to_owned))
            }
            _ => ("uncertain", None),
        };
        self.database()?.execute(
            "UPDATE executions SET status=?2,log_id=?3 WHERE id=?1",
            params![execution, status, log],
        )?;
        match result {
            Ok(value) if status == "completed" => {
                Ok(json!({"execution_id":execution,"data":value["data"],"log_id":log}))
            }
            _ => Ok(
                json!({"execution_id":execution,"outcome":"uncertain","error":"The external outcome could not be confirmed. Inspect the account and execution history before retrying."}),
            ),
        }
    }
}
