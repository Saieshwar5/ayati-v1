use super::VmManager;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

impl VmManager {
    // This never boots a VM implicitly. The owner/worker chooses wake separately.
    pub async fn browser(&self, request: Value) -> Result<Value> {
        let process = self.process.lock().await;
        let vm = process
            .as_ref()
            .context("Start the agent VM before using its browser")?;
        ensure!(
            vm.config.browser_version.as_deref() == Some("0.39.1"),
            "Prepare a new browser-enabled VM first"
        );
        let mut command = vm.config.ssh(vm.ssh_port);
        let mut child = command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let input =
            json!({"version":1,"agent_id":vm.config.agent_id,"op":"browser","browser":request});
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();
        // Do not hold the VM process mutex while a command drains or waits. Stop
        // must remain able to terminate the owned VM and every browser process.
        drop(process);
        let exchange = async {
            stdin.write_all(format!("{input}\n").as_bytes()).await?;
            stdin.shutdown().await?;
            let mut bytes = Vec::new();
            (&mut stdout)
                .take(ayati_guest::browser_mcp::LIMIT + 1)
                .read_to_end(&mut bytes)
                .await?;
            ensure!(
                bytes.len() as u64 <= ayati_guest::browser_mcp::LIMIT
                    && child.wait().await?.success(),
                "Browser bridge failed; outcome may be uncertain"
            );
            let response: Value = serde_json::from_slice(&bytes)?;
            ensure!(
                response.get("error").is_none(),
                "{}",
                response["error"].as_str().unwrap_or("Browser failed")
            );
            Ok::<_, anyhow::Error>(response)
        };
        tokio::time::timeout(Duration::from_secs(60), exchange)
            .await
            .context("Browser outcome uncertain; no automatic retry")?
    }
}
