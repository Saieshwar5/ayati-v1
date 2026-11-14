//! Commands and files on a workspace's own computer: queued for its connector,
//! answered by it, and waited for here. The same four things Cuckoo's own
//! computers do (run, write, read, list), so the agent's tools, the Programs
//! rule and Activity are unchanged. Nothing is ever retried: work the machine
//! didn't answer in time is abandoned and reported as unconfirmed.
use super::super::computers::Ready;
use crate::identity::computers::Ran;
use crate::identity::postgres::*;
use base64::Engine;
use serde_json::json;
use std::time::Duration;

/// Extra time a connector may take on top of a command's own limit.
#[cfg(not(test))]
const SLACK: Duration = Duration::from_secs(30);
#[cfg(test)]
const SLACK: Duration = Duration::from_secs(3);

impl PgStore {
    /// Sends one piece of work to the machine and waits for its answer.
    async fn relay(
        &self,
        ready: &Ready,
        kind: &str,
        request: Value,
        wait: Duration,
    ) -> Result<Value> {
        let shared = ready.shared.as_ref().context("Not a workspace computer")?;
        let id = uuid::Uuid::new_v4().to_string();
        execute(&self.db, "INSERT INTO relay_workspace_computer_calls(id,workspace,computer,agent,kind,request) SELECT $1,workspace,id,$3,$4,$5 FROM relay_workspace_computers WHERE id=$2",
            vec![id.clone().into(), shared.computer.clone().into(), ready.awake.token.clone().into(), kind.into(), request.into()]).await?;
        let started = std::time::Instant::now();
        loop {
            let row = query(
                &self.db,
                "SELECT state,result FROM relay_workspace_computer_calls WHERE id=$1",
                vec![id.clone().into()],
            )
            .await?
            .context("The work for the computer disappeared")?;
            let state: String = row.try_get("", "state")?;
            if state == "done" {
                let result: Value = row
                    .try_get::<Option<Value>>("", "result")?
                    .unwrap_or_default();
                if let Some(error) = result["error"].as_str() {
                    anyhow::bail!("{error}");
                }
                return Ok(result);
            }
            if state == "abandoned" {
                anyhow::bail!("The computer was removed");
            }
            if started.elapsed() > wait {
                execute(&self.db, "UPDATE relay_workspace_computer_calls SET state='abandoned',done_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state IN ('queued','taken')",
                    vec![id.into()]).await?;
                anyhow::bail!("{} didn't answer in time", shared.name);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Runs a command on the agent's computer: Cuckoo's, or the workspace's.
    pub(in crate::identity::postgres) async fn computer_exec(
        &self,
        ready: &Ready,
        command: &str,
        cwd: &str,
        timeout: Duration,
    ) -> Result<Ran> {
        if ready.shared.is_none() {
            return ready
                .routed
                .provider
                .run(&ready.call(), &ready.awake, command, cwd, timeout)
                .await;
        }
        let result = self
            .relay(
                ready,
                "run",
                json!({"command":command,"cwd":cwd,"seconds":timeout.as_secs()}),
                timeout + SLACK,
            )
            .await?;
        Ok(Ran {
            exit_code: result["exit_code"].as_i64().unwrap_or(-1) as i32,
            stdout: result["stdout"].as_str().unwrap_or_default().to_owned(),
            stderr: result["stderr"].as_str().unwrap_or_default().to_owned(),
            output_bytes: result["output_bytes"].as_u64().unwrap_or(0),
            timed_out: result["timed_out"] == true,
        })
    }

    pub(in crate::identity::postgres) async fn computer_put(
        &self,
        ready: &Ready,
        path: &str,
        bytes: Vec<u8>,
    ) -> Result<()> {
        if ready.shared.is_none() {
            return ready
                .routed
                .provider
                .write(&ready.call(), &ready.awake, path, bytes)
                .await;
        }
        let content = base64::engine::general_purpose::STANDARD.encode(bytes);
        self.relay(
            ready,
            "write",
            json!({"path":path,"content_b64":content}),
            SLACK,
        )
        .await?;
        Ok(())
    }

    pub(in crate::identity::postgres) async fn computer_get(
        &self,
        ready: &Ready,
        path: &str,
    ) -> Result<Vec<u8>> {
        if ready.shared.is_none() {
            return ready
                .routed
                .provider
                .read(&ready.call(), &ready.awake, path)
                .await;
        }
        let result = self
            .relay(ready, "read", json!({"path":path}), SLACK)
            .await?;
        base64::engine::general_purpose::STANDARD
            .decode(result["content_b64"].as_str().unwrap_or_default())
            .context("The computer's file came back damaged")
    }

    /// A folder's entries as `type\tbytes\tchanged\tname` lines, newest name order.
    /// Cuckoo's computers list with `find`; a connector lists natively, because
    /// not every machine has GNU find.
    pub(in crate::identity::postgres) async fn computer_ls(
        &self,
        ready: &Ready,
        path: &str,
        find: &str,
    ) -> Result<String> {
        if ready.shared.is_none() {
            let home = crate::identity::computers::HOME;
            return Ok(self
                .computer_exec(ready, find, home, Duration::from_secs(30))
                .await?
                .stdout);
        }
        let result = self
            .relay(ready, "list", json!({"path":path}), SLACK)
            .await?;
        Ok(result["stdout"].as_str().unwrap_or_default().to_owned())
    }
}
