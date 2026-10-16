use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

pub struct Upstream {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    sequence: u64,
}

impl Upstream {
    pub async fn start() -> Result<Self> {
        let version = Command::new("/opt/ayati/browser/agent-browser")
            .arg("--version")
            .output()
            .await?;
        ensure!(
            version.status.success()
                && String::from_utf8_lossy(&version.stdout).trim() == "agent-browser 0.39.1",
            "Pinned browser version mismatch"
        );
        let mut child = Command::new("/opt/ayati/browser/agent-browser")
            .args(["mcp", "--tools", "all"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/var/lib/ayati-browser")
            .env("AGENT_BROWSER_SOCKET_DIR", "/run/ayati-browser/private")
            .env("AGENT_BROWSER_SESSION", "ayati")
            .env("AGENT_BROWSER_EXECUTABLE_PATH", "/opt/ayati/browser/chrome")
            .env("AGENT_BROWSER_PROFILE", "/var/lib/ayati-browser/profile")
            .env("AGENT_BROWSER_DOWNLOAD_PATH", super::browser_policy::TRANSFERS)
            .env("AGENT_BROWSER_PROXY", "http://10.0.2.101:8081")
            .env("AGENT_BROWSER_PROXY_BYPASS", "<-loopback>")
            .env("AGENT_BROWSER_NO_WEBMCP", "1")
            .env("AGENT_BROWSER_DEFAULT_TIMEOUT", "15000")
            .env("AGENT_BROWSER_IDLE_TIMEOUT_MS", "0")
            .env("AGENT_BROWSER_STREAM_PORT", "9223")
            .env("AGENT_BROWSER_ARGS", "--disable-quic,--disable-background-networking,--disable-sync,--no-first-run,--disable-breakpad")
            .current_dir("/var/lib/ayati-browser")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .kill_on_drop(true).process_group(0).spawn()?;
        let mut this = Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            sequence: 0,
        };
        this.rpc("initialize", json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"ayati-browser-broker","version":"1"}})).await?;
        this.input
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .await?;
        Ok(this)
    }

    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        self.sequence += 1;
        self.input
            .write_all(
                format!(
                    "{}\n",
                    json!({"jsonrpc":"2.0","id":self.sequence,"method":method,"params":params})
                )
                .as_bytes(),
            )
            .await?;
        self.input.flush().await?;
        let mut line = Vec::new();
        tokio::time::timeout(
            Duration::from_secs(45),
            (&mut self.output)
                .take(super::browser_mcp::LIMIT + 1)
                .read_until(b'\n', &mut line),
        )
        .await
        .context("Browser outcome unknown; no retry")??;
        ensure!(
            line.len() as u64 <= super::browser_mcp::LIMIT && line.ends_with(b"\n"),
            "Invalid upstream reply"
        );
        let reply: Value = serde_json::from_slice(&line)?;
        ensure!(
            reply["id"] == self.sequence && reply.get("error").is_none(),
            "Upstream browser protocol failed"
        );
        reply
            .get("result")
            .cloned()
            .context("Missing upstream result")
    }

    pub async fn call(&mut self, tool: &str, arguments: Value) -> Result<Value> {
        self.rpc("tools/call", json!({"name":tool,"arguments":arguments}))
            .await
    }
}

impl Drop for Upstream {
    fn drop(&mut self) {
        if let Some(id) = self.child.id() {
            unsafe {
                libc::kill(-(id as i32), libc::SIGKILL);
            }
        }
        let _ = self.child.start_kill();
    }
}
