//! E2B (Firecracker microVMs). Control calls go to its API with the key; commands and
//! files go to the computer through `sandbox.e2b.app` with the computer's own access
//! token. Commands use Connect RPC with JSON messages, so no protobuf is needed.
//! Checked against the live service on 2026-11-08 (runtime/agent-computer-providers.md).
use super::{Awake, Call, Internet, KEPT_BYTES, Provider, Ran, Spec};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::time::Duration;

pub(crate) struct E2b;

/// The computer's own agent (envd) listens on this port.
const ENVD_PORT: &str = "49983";
/// Commands and files run as this user, whose home is [`super::HOME`].
const USER: &str = "Basic dXNlcjo="; // base64 of "user:"

fn client(timeout: Duration) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()?)
}

/// The next whole Connect message in `buffer`, with its flags; flag 2 ends the stream.
fn take_message(buffer: &mut Vec<u8>) -> Option<(u8, Value)> {
    if buffer.len() < 5 {
        return None;
    }
    let size = u32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]) as usize;
    if buffer.len() < 5 + size {
        return None;
    }
    let flags = buffer[0];
    let message = serde_json::from_slice(&buffer[5..5 + size]).unwrap_or(Value::Null);
    buffer.drain(..5 + size);
    Some((flags, message))
}

/// Decoded output bytes of one stream (stdout or stderr) in a process event.
fn output(event: &Value, stream: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    let data = event["data"][stream].as_str()?;
    Some(
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap_or_default(),
    )
}

/// The computer's file address for one path, as its own user.
fn files_url(base: &str, path: &str) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(&format!("{base}/files")).context("Invalid computer URL")?;
    url.query_pairs_mut()
        .append_pair("path", path)
        .append_pair("username", "user");
    Ok(url)
}

fn network(internet: &Internet) -> Value {
    match internet {
        Internet::None => json!({"allowOut": [], "denyOut": ["0.0.0.0/0"]}),
        Internet::Only(sites) => json!({"allowOut": sites, "denyOut": ["0.0.0.0/0"]}),
        Internet::Any => json!({"allowOut": [], "denyOut": []}),
    }
}

async fn checked(response: reqwest::Response, what: &str) -> Result<Value> {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    ensure!(
        status.is_success(),
        "{what} failed ({status}): {}",
        text.chars().take(200).collect::<String>()
    );
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
}

fn awake(answer: &Value) -> Result<Awake> {
    Ok(Awake {
        id: answer["sandboxID"]
            .as_str()
            .context("E2B gave no computer id")?
            .to_owned(),
        token: answer["envdAccessToken"]
            .as_str()
            .context("E2B gave no access token")?
            .to_owned(),
    })
}

/// One Connect envelope: a flag byte, a big-endian length, then the JSON message.
fn envelope(message: &Value) -> Vec<u8> {
    let data = serde_json::to_vec(message).unwrap_or_default();
    let mut out = Vec::with_capacity(data.len() + 5);
    out.push(0);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&data);
    out
}

/// Adds output to what's kept, up to [`KEPT_BYTES`].
fn keep(kept: &mut Vec<u8>, data: &[u8]) {
    let room = KEPT_BYTES.saturating_sub(kept.len());
    kept.extend_from_slice(&data[..data.len().min(room)]);
}

#[async_trait::async_trait]
impl Provider for E2b {
    fn name(&self) -> &'static str {
        "e2b"
    }
    fn api_url(&self) -> &'static str {
        "https://api.e2b.app"
    }
    fn computer_url(&self) -> &'static str {
        "https://sandbox.e2b.app"
    }

    async fn create(&self, call: &Call<'_>, spec: &Spec<'_>) -> Result<Awake> {
        let body = json!({
            "templateID": spec.image,
            "timeout": spec.awake_seconds,
            "autoPause": true,
            "autoResume": {"enabled": false},
            "network": network(spec.internet),
            "metadata": {"cuckoo": spec.label},
        });
        let response = client(Duration::from_secs(60))?
            .post(format!("{}/v2/sandboxes", call.api))
            .header("X-API-Key", call.key)
            .json(&body)
            .send()
            .await
            .context("E2B could not be reached")?;
        awake(&checked(response, "Creating the computer").await?)
    }

    async fn wake(&self, call: &Call<'_>, id: &str, awake_seconds: u64) -> Result<Awake> {
        let response = client(Duration::from_secs(60))?
            .post(format!("{}/sandboxes/{id}/connect", call.api))
            .header("X-API-Key", call.key)
            .json(&json!({"timeout": awake_seconds}))
            .send()
            .await
            .context("E2B could not be reached")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("The computer no longer exists at its provider");
        }
        awake(&checked(response, "Waking the computer").await?)
    }

    async fn sleep(&self, call: &Call<'_>, id: &str) -> Result<()> {
        let response = client(Duration::from_secs(60))?
            .post(format!("{}/sandboxes/{id}/pause", call.api))
            .header("X-API-Key", call.key)
            .send()
            .await
            .context("E2B could not be reached")?;
        // Already asleep is fine; one that is gone is started again on its next use.
        if [
            reqwest::StatusCode::CONFLICT,
            reqwest::StatusCode::NOT_FOUND,
        ]
        .contains(&response.status())
        {
            return Ok(());
        }
        checked(response, "Putting the computer to sleep").await?;
        Ok(())
    }

    async fn delete(&self, call: &Call<'_>, id: &str) -> Result<()> {
        let response = client(Duration::from_secs(60))?
            .delete(format!("{}/sandboxes/{id}", call.api))
            .header("X-API-Key", call.key)
            .send()
            .await
            .context("E2B could not be reached")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        checked(response, "Deleting the computer").await?;
        Ok(())
    }

    async fn set_internet(&self, call: &Call<'_>, id: &str, internet: &Internet) -> Result<()> {
        let response = client(Duration::from_secs(30))?
            .put(format!("{}/sandboxes/{id}/network", call.api))
            .header("X-API-Key", call.key)
            .json(&network(internet))
            .send()
            .await
            .context("E2B could not be reached")?;
        checked(response, "Changing the computer's internet").await?;
        Ok(())
    }

    async fn run(
        &self,
        call: &Call<'_>,
        computer: &Awake,
        command: &str,
        cwd: &str,
        timeout: Duration,
    ) -> Result<Ran> {
        // `timeout` inside the computer stops the command itself, not just our wait.
        // E2B refuses a working folder that doesn't exist, so the command makes it.
        let seconds = timeout.as_secs().max(1).to_string();
        let folder = format!("'{}'", cwd.replace('\'', "'\\''"));
        let script = format!("mkdir -p {folder} && cd {folder} && {command}");
        let start = json!({"process": {
            "cmd": "/usr/bin/timeout",
            "args": ["-k", "5", seconds, "/bin/bash", "-l", "-c", script],
            "envs": {},
            "cwd": super::HOME,
        }, "stdin": false});
        let mut response = client(timeout + Duration::from_secs(20))?
            .post(format!("{}/process.Process/Start", call.computers))
            .header("E2b-Sandbox-Id", &computer.id)
            .header("E2b-Sandbox-Port", ENVD_PORT)
            .header("X-Access-Token", &computer.token)
            .header("Authorization", USER)
            .header("Content-Type", "application/connect+json")
            .header("Connect-Protocol-Version", "1")
            .body(envelope(&start))
            .send()
            .await
            .context("The computer could not be reached")?;
        ensure!(
            response.status().is_success(),
            "Running the command failed ({})",
            response.status()
        );
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut total = 0u64;
        let mut exit_code = None;
        let mut buffer: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .context("The command's output stopped")?
        {
            buffer.extend_from_slice(&chunk);
            while let Some((flags, message)) = take_message(&mut buffer) {
                if flags & 2 != 0 {
                    // End of stream: an error here means the command didn't run.
                    if let Some(error) = message.get("error") {
                        bail!(
                            "The computer refused the command: {}",
                            error["message"].as_str().unwrap_or("unknown error")
                        );
                    }
                    continue;
                }
                let event = &message["event"];
                for (name, kept) in [("stdout", &mut stdout), ("stderr", &mut stderr)] {
                    if let Some(bytes) = output(event, name) {
                        total += bytes.len() as u64;
                        keep(kept, &bytes);
                    }
                }
                if event.get("end").is_some() {
                    exit_code = Some(event["end"]["exitCode"].as_i64().unwrap_or(0) as i32);
                }
            }
        }
        let exit_code = exit_code.context("The command ended without an exit code")?;
        Ok(Ran {
            exit_code,
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            output_bytes: total,
            // `timeout` exits 124 when it stopped the command, 137 after the kill.
            timed_out: exit_code == 124 || exit_code == 137,
        })
    }

    async fn stream(
        &self,
        call: &Call<'_>,
        computer: &Awake,
        command: &str,
        tag: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<Vec<u8>>> {
        let start = json!({"process": {
            "cmd": "/bin/bash", "args": ["-l", "-c", command], "envs": {}, "cwd": super::HOME,
        }, "tag": tag, "stdin": true});
        // No overall timeout: the stream lasts as long as the program; it is stopped by
        // closing its input.
        let mut response = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(20))
            .build()?
            .post(format!("{}/process.Process/Start", call.computers))
            .header("E2b-Sandbox-Id", &computer.id)
            .header("E2b-Sandbox-Port", ENVD_PORT)
            .header("X-Access-Token", &computer.token)
            .header("Authorization", USER)
            .header("Content-Type", "application/connect+json")
            .header("Connect-Protocol-Version", "1")
            .body(envelope(&start))
            .send()
            .await
            .context("The computer could not be reached")?;
        ensure!(
            response.status().is_success(),
            "Starting the program failed ({})",
            response.status()
        );
        let (sender, receiver) = tokio::sync::mpsc::channel(64);
        tokio::spawn(async move {
            let mut buffer = Vec::new();
            while let Ok(Some(chunk)) = response.chunk().await {
                buffer.extend_from_slice(&chunk);
                while let Some((flags, message)) = take_message(&mut buffer) {
                    if flags & 2 != 0 {
                        return;
                    }
                    if let Some(bytes) = output(&message["event"], "stdout")
                        && sender.send(bytes).await.is_err()
                    {
                        return;
                    }
                }
            }
        });
        Ok(receiver)
    }

    async fn send(&self, call: &Call<'_>, computer: &Awake, tag: &str, bytes: &[u8]) -> Result<()> {
        use base64::Engine as _;
        let input = base64::engine::general_purpose::STANDARD.encode(bytes);
        let response = client(Duration::from_secs(20))?
            .post(format!("{}/process.Process/SendInput", call.computers))
            .header("E2b-Sandbox-Id", &computer.id)
            .header("E2b-Sandbox-Port", ENVD_PORT)
            .header("X-Access-Token", &computer.token)
            .header("Authorization", USER)
            .header("Connect-Protocol-Version", "1")
            .json(&json!({"process": {"tag": tag}, "input": {"stdin": input}}))
            .send()
            .await
            .context("The computer could not be reached")?;
        checked(response, "Sending to the program").await?;
        Ok(())
    }

    async fn write(
        &self,
        call: &Call<'_>,
        computer: &Awake,
        path: &str,
        bytes: Vec<u8>,
    ) -> Result<()> {
        let response = client(Duration::from_secs(60))?
            .post(files_url(call.computers, path)?)
            .header("E2b-Sandbox-Id", &computer.id)
            .header("E2b-Sandbox-Port", ENVD_PORT)
            .header("X-Access-Token", &computer.token)
            .header("Authorization", USER)
            .header("Content-Type", "application/octet-stream")
            .body(bytes)
            .send()
            .await
            .context("The computer could not be reached")?;
        checked(response, "Saving the file").await?;
        Ok(())
    }

    async fn read(&self, call: &Call<'_>, computer: &Awake, path: &str) -> Result<Vec<u8>> {
        let response = client(Duration::from_secs(60))?
            .get(files_url(call.computers, path)?)
            .header("E2b-Sandbox-Id", &computer.id)
            .header("E2b-Sandbox-Port", ENVD_PORT)
            .header("X-Access-Token", &computer.token)
            .header("Authorization", USER)
            .send()
            .await
            .context("The computer could not be reached")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("No such file on the computer");
        }
        ensure!(
            response.status().is_success(),
            "Reading the file failed ({})",
            response.status()
        );
        Ok(response.bytes().await?.to_vec())
    }
}
