use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::json;
use std::{path::Path, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

const ROOT: &str = "/var/lib/ayati";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    agent_id: String,
    op: String,
    model: Option<String>,
    token: Option<String>,
    value: Option<String>,
    port: Option<u16>,
    browser: Option<serde_json::Value>,
    run: Option<String>,
    memory_token: Option<String>,
}

pub async fn serve() -> Result<()> {
    let mut input = BufReader::new(tokio::io::stdin());
    let mut line = Vec::new();
    (&mut input)
        .take(super::browser_mcp::LIMIT + 1)
        .read_until(b'\n', &mut line)
        .await?;
    ensure!(
        line.len() as u64 <= super::browser_mcp::LIMIT && line.ends_with(b"\n"),
        "Invalid bridge initialization"
    );
    let request: Request = serde_json::from_slice(&line).context("Invalid bridge request shape")?;
    let expected = std::fs::read_to_string("/etc/ayati/agent-id")?;
    ensure!(
        request.version == 1 && request.agent_id == expected.trim(),
        "Wrong bridge version or agent identity"
    );
    match request.op.as_str() {
        "probe" => {
            println!(
                "{}",
                json!({"ready":true,"agent_id":request.agent_id,"kernel":std::fs::read_to_string("/proc/sys/kernel/osrelease")?.trim(),"bridge_version":"0.1.0"})
            );
            Ok(())
        }
        "storage_probe" => {
            let path = Path::new(ROOT).join("workspace/.ayati-storage-probe");
            if let Some(value) = request.value {
                ensure!(
                    value.len() <= 64
                        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                    "Invalid probe value"
                );
                std::fs::write(&path, value)?;
            }
            println!(
                "{}",
                json!({"agent_id":request.agent_id,"value":std::fs::read_to_string(path).unwrap_or_default()})
            );
            Ok(())
        }
        "network_probe" => {
            let port = request.port.context("A host listener port is required")?;
            let reachable = |address| async move {
                matches!(
                    tokio::time::timeout(
                        std::time::Duration::from_millis(500),
                        tokio::net::TcpStream::connect(address)
                    )
                    .await,
                    Ok(Ok(_))
                )
            };
            println!(
                "{}",
                json!({"host_gateway_accessible":reachable((std::net::Ipv4Addr::new(10,0,2,2),port)).await,"metadata_accessible":reachable((std::net::Ipv4Addr::new(169,254,169,254),80)).await})
            );
            Ok(())
        }
        "acp" => run_goose(request, input).await,
        "browser_probe" => {
            let release: serde_json::Value =
                serde_json::from_slice(&std::fs::read("/opt/ayati/browser/release.json")?)?;
            println!(
                "{}",
                json!({"runtime_cannot_list_profile":std::fs::read_dir("/var/lib/ayati-browser/profile").is_err(),"runtime_cannot_list_private_sockets":std::fs::read_dir("/run/ayati-browser/private").is_err(),"agent_browser_version":release["agent_browser"]["version"],"chrome_version":release["chrome"]["version"]})
            );
            Ok(())
        }
        "browser" => {
            println!(
                "{}",
                super::browser_mcp::exchange(&request.browser.context("Missing browser request")?)
                    .await?
            );
            Ok(())
        }
        _ => anyhow::bail!("Unsupported bridge operation"),
    }
}

struct OwnedGoose(tokio::process::Child, i32);
impl Drop for OwnedGoose {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.1, libc::SIGKILL);
        }
        let _ = self.0.start_kill();
    }
}

async fn run_goose(request: Request, mut input: BufReader<tokio::io::Stdin>) -> Result<()> {
    let run = request.run.unwrap_or_default();
    ensure!(
        run.len() <= 64 && run.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
        "Invalid run identity"
    );
    let model = request.model.context("Model is required")?;
    let token = request.token.context("Model access token is required")?;
    ensure!(
        !model.is_empty() && model.len() <= 256 && !model.chars().any(char::is_control),
        "Invalid model"
    );
    ensure!(
        token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()),
        "Invalid model access token"
    );
    let version = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        Command::new("/opt/ayati/goose")
            .arg("--version")
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    ensure!(
        version.status.success() && String::from_utf8_lossy(&version.stdout).trim() == "1.54.0",
        "Guest Goose version differs from the pinned artifact"
    );
    let mut hangup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut command = Command::new("/opt/ayati/goose");
    command
        .arg("acp")
        .current_dir(format!("{ROOT}/workspace"))
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("GOOSE_PATH_ROOT", format!("{ROOT}/goose"))
        .env("GOOSE_PROVIDER", "openai")
        .env("AYATI_BROWSER_RUN", run)
        .env("GOOSE_MODEL", model)
        .env("OPENAI_HOST", "http://10.0.2.100:8080")
        .env("OPENAI_BASE_PATH", "v1/chat/completions")
        .env("OPENAI_API_KEY", token)
        .env("GOOSE_MODE", "approve")
        .env("GOOSE_DISABLE_KEYRING", "1")
        .env("GOOSE_DISABLE_SESSION_NAMING", "true")
        .env("DO_NOT_TRACK", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .process_group(0);
    if let Some(token) = request.memory_token {
        ensure!(
            token.len() == 36 && token.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
            "Invalid memory run token"
        );
        command
            .env("AYATI_MEMORY_TOKEN", token)
            .env("AYATI_MEMORY_ADDRESS", "10.0.2.100:8080");
    }
    let mut child = command.spawn()?;
    let mut sink = child.stdin.take().unwrap();
    let mut source = child.stdout.take().unwrap();
    let mut owned = OwnedGoose(child, 0);
    owned.1 = owned.0.id().unwrap() as i32;
    let mut output = tokio::io::stdout();
    output
        .write_all(
            format!(
                "{}\n",
                json!({"ready":true,"agent_id":request.agent_id,"version":1})
            )
            .as_bytes(),
        )
        .await?;
    output.flush().await?;
    // SSH EOF/disconnect terminates the runtime group. No remote shell or
    // arbitrary filesystem/process operations are exposed by this bridge.
    tokio::select! {
        _ = hangup.recv() => {},
        _ = terminate.recv() => {},
        result = tokio::io::copy(&mut input, &mut sink) => { result?; }
        result = tokio::io::copy(&mut source, &mut output) => { result?; }
        result = owned.0.wait() => { ensure!(result?.success(), "Guest runtime exited unexpectedly"); }
    }
    Ok(())
}
