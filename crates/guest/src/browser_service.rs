//! One serialized broker owns the browser and its protected profile.
use super::{
    browser_mcp::LIMIT,
    browser_policy::{self, TRANSFERS},
    browser_upstream::Upstream,
};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, path::Path, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};

const OWNER: &str = "/var/lib/ayati-browser/owner.json";
const FRAME: &str = "/run/ayati-browser/private/frame.jpg";
const FILE_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub op: String,
    pub token: Option<String>,
    pub tool: Option<String>,
    pub arguments: Option<Value>,
    pub file: Option<String>,
    pub data: Option<String>,
}

struct Broker {
    upstream: Option<Upstream>,
    owner: Option<String>,
    enabled: bool,
    run: Option<String>,
    fresh: bool,
    poisoned: bool,
    parked: bool,
    interaction_pending: bool,
    closed: bool,
}

impl Broker {
    fn check_owner(&self, request: &Request) -> Result<()> {
        ensure!(
            self.owner.is_some() && self.owner == request.token,
            "This browser controller has expired"
        );
        Ok(())
    }

    async fn call(&mut self, tool: &str, arguments: Value) -> Result<Value> {
        ensure!(
            !self.poisoned,
            "Browser outcome uncertain; Stop the VM and inspect before continuing"
        );
        if self.upstream.is_none() {
            self.upstream = Some(Upstream::start().await?);
        }
        let result = self.upstream.as_mut().unwrap().call(tool, arguments).await;
        if result.as_ref().map_or(true, |r| {
            r.pointer("/structuredContent/response/code") == Some(&json!("outcome_unknown"))
        }) {
            self.poisoned = true;
            self.enabled = false;
        }
        result
    }

    fn save_owner(&mut self, owner: Option<String>) -> Result<()> {
        let temp = format!("{OWNER}.new");
        std::fs::write(&temp, serde_json::to_vec(&owner)?)?;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
        std::fs::File::open(&temp)?.sync_all()?;
        std::fs::rename(temp, OWNER)?;
        std::fs::File::open("/var/lib/ayati-browser")?.sync_all()?;
        self.owner = owner;
        Ok(())
    }

    async fn handle(&mut self, request: Request) -> Result<Value> {
        match request.op.as_str() {
            "status" => {
                return Ok(
                    json!({"owner":self.owner.is_some(),"enabled":self.enabled,"uncertain":self.poisoned,"profile_initialized":Path::new("/var/lib/ayati-browser/profile/Local State").exists(),"interaction_pending":self.interaction_pending,"closed":self.closed}),
                );
            }
            "arm" => {
                ensure!(!self.parked, "Browser is shutting down");
                ensure!(
                    self.owner.is_none() && !self.poisoned,
                    "Owner controls the browser or its outcome is uncertain"
                );
                let run = request.token.context("Missing browser run identity")?;
                self.run = Some(run);
                self.enabled = true;
                self.closed = false;
                self.fresh = false;
            }
            "disarm" => {
                self.enabled = false;
                self.run = None;
                self.fresh = false;
            }
            "take" => {
                ensure!(!self.parked, "Browser is shutting down");
                let token = request.token.context("Missing owner token")?;
                ensure!(
                    token.len() == 36 && token.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
                    "Invalid owner token"
                );
                ensure!(
                    self.owner.as_ref().is_none_or(|t| *t == token),
                    "Browser already controlled by another owner tab"
                );
                ensure!(
                    !self.poisoned,
                    "Cannot transfer uncertain browser work; Stop the VM first"
                );
                self.enabled = false;
                self.closed = false;
                self.fresh = false;
                // The mutex drains the preceding command before this acknowledgement.
                self.save_owner(Some(token))?;
            }
            "resume" => {
                ensure!(!self.parked, "Browser is shutting down");
                self.check_owner(&request)?;
                // Clear credential fields before any new agent capture. The fixed
                // script cannot be selected or edited by Goose or a website.
                let cleared = self.call("agent_browser_eval", json!({"script":"document.querySelectorAll('input[type=password],input[autocomplete=one-time-code],input[autocomplete=current-password],input[autocomplete=new-password]').forEach(e=>{e.value=''}); true"})).await?;
                ensure!(
                    cleared["isError"] != true,
                    "Could not clear credential fields; owner control retained"
                );
                let observed = self
                    .call(
                        "agent_browser_snapshot",
                        json!({"interactive":true,"compact":true}),
                    )
                    .await?;
                ensure!(
                    observed["isError"] != true,
                    "Could not observe browser; owner control retained"
                );
                self.save_owner(None)?;
                self.fresh = false;
                // Agent use stays disabled until the host explicitly arms a run.
                self.enabled = false;
            }
            "frame" => {
                ensure!(!self.parked, "Browser is shutting down");
                ensure!(
                    !self.closed,
                    "Browser session is closed; take control or submit new work to reopen it"
                );
                // Viewing is allowed without control; captures never go to Goose.
                // Owner frames are memory-only and never enter action/history logs.
                if self.owner.is_some() {
                    self.check_owner(&request)?;
                }
                let frame = self
                    .call(
                        "agent_browser_screenshot",
                        json!({"path":FRAME,"format":"jpeg","quality":45}),
                    )
                    .await?;
                let _ = std::fs::remove_file(FRAME);
                return Ok(frame);
            }
            "input" => {
                ensure!(!self.parked, "Browser is shutting down");
                self.check_owner(&request)?;
                let tool = request.tool.context("Missing input operation")?;
                let arguments = request.arguments.unwrap_or(json!({}));
                validate_input(&tool, &arguments)?;
                self.interaction_pending = true;
                return self.call(&tool, arguments).await;
            }
            "put_file" => {
                self.check_owner(&request)?;
                let name = request.file.context("Missing filename")?;
                browser_policy::filename(&name)?;
                let bytes = STANDARD.decode(request.data.context("Missing file")?)?;
                ensure!(bytes.len() <= FILE_LIMIT, "Transfer exceeds 2 MiB");
                let path = Path::new(TRANSFERS).join(name);
                use std::os::unix::fs::OpenOptionsExt;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)?;
                std::io::Write::write_all(&mut file, &bytes)?;
                self.interaction_pending = true;
            }
            "get_file" => {
                self.check_owner(&request)?;
                let name = request.file.context("Missing filename")?;
                browser_policy::filename(&name)?;
                let path = Path::new(TRANSFERS).join(name);
                let metadata = std::fs::symlink_metadata(&path)?;
                ensure!(
                    metadata.is_file() && metadata.len() <= FILE_LIMIT as u64,
                    "Invalid or oversized transfer"
                );
                return Ok(json!({"data":STANDARD.encode(std::fs::read(path)?)}));
            }
            "agent" => {
                ensure!(!self.parked, "Browser is shutting down");
                ensure!(
                    request.token.is_some() && self.run == request.token,
                    "Stale browser run identity"
                );
                ensure!(
                    self.owner.is_none() && self.enabled && !self.poisoned,
                    "Browser stopped or waiting for owner"
                );
                let tool = request.tool.context("Missing browser operation")?;
                let mut args = request.arguments.unwrap_or(json!({}));
                browser_policy::validate(&tool, &args)?;
                if self.closed {
                    ensure!(
                        tool == "agent_browser_open",
                        "Open a website to begin a new browser session"
                    );
                    self.closed = false;
                }
                // Starting a new navigation is valid without a snapshot. All
                // interactions after navigation/takeover need fresh references.
                ensure!(
                    self.fresh
                        || [
                            "agent_browser_snapshot",
                            "agent_browser_open",
                            "agent_browser_tab_list",
                            "agent_browser_close"
                        ]
                        .contains(&tool.as_str()),
                    "Take a fresh snapshot before interacting"
                );
                if tool == "agent_browser_snapshot" {
                    args = json!({"interactive":true,"compact":true});
                } else if tool == "agent_browser_screenshot" {
                    args = json!({"path":FRAME,"format":"jpeg","quality":45});
                } else if tool == "agent_browser_tab_new" {
                    let tabs = self.call("agent_browser_tab_list", json!({})).await?;
                    let count = tabs
                        .pointer("/structuredContent/response/data/tabs")
                        .and_then(Value::as_array)
                        .map(Vec::len);
                    ensure!(count.is_some_and(|n| n < 4), "Browser tab limit reached");
                } else if tool == "agent_browser_fill" {
                    let check = self.call("agent_browser_eval", json!({"script":"!!document.querySelector('input[type=password],input[autocomplete=one-time-code]')"})).await?;
                    ensure!(
                        check.pointer("/structuredContent/response/data/result")
                            == Some(&json!(false)),
                        "Ask the owner to complete login in takeover mode"
                    );
                }
                if let Some(name) = args.get("file").and_then(Value::as_str).map(str::to_owned) {
                    let path = Path::new(TRANSFERS).join(name);
                    args.as_object_mut().unwrap().remove("file");
                    if tool == "agent_browser_upload" {
                        let metadata = std::fs::symlink_metadata(&path)?;
                        ensure!(
                            metadata.is_file() && metadata.len() <= FILE_LIMIT as u64,
                            "Invalid transfer file"
                        );
                        args["files"] = json!([path]);
                    } else {
                        ensure!(
                            !path.exists(),
                            "Download already exists; inspect it rather than retrying"
                        );
                        args["path"] = json!(path);
                    }
                }
                if [
                    "agent_browser_fill",
                    "agent_browser_upload",
                    "agent_browser_click",
                    "agent_browser_press",
                    "agent_browser_download",
                ]
                .contains(&tool.as_str())
                {
                    self.interaction_pending = true;
                }
                let result = self.call(&tool, args.clone()).await?;
                if tool == "agent_browser_close" && result["isError"] != true {
                    self.interaction_pending = false;
                    self.closed = true;
                    self.upstream.take();
                }
                self.fresh = tool == "agent_browser_snapshot" && result["isError"] != true;
                let _ = std::fs::remove_file(FRAME);
                if tool == "agent_browser_download" && result["isError"] != true {
                    let path = args["path"].as_str().unwrap();
                    if std::fs::metadata(path)?.len() > FILE_LIMIT as u64 {
                        std::fs::remove_file(path)?;
                        anyhow::bail!(
                            "Download exceeded 2 MiB and was removed; do not automatically retry"
                        );
                    }
                }
                ensure!(
                    result.to_string().len() <= 3 * 1024 * 1024,
                    "Browser observation exceeds its limit"
                );
                return Ok(result);
            }
            "park" => {
                self.parked = true;
                self.enabled = false;
                self.run = None;
                self.fresh = false;
                if let Some(upstream) = &mut self.upstream {
                    let closed = upstream.call("agent_browser_close", json!({})).await?;
                    ensure!(
                        closed["isError"] != true,
                        "Browser profile flush was not confirmed"
                    );
                }
                self.upstream.take();
            }
            "finish" => {
                self.check_owner(&request)?;
                self.closed = true;
                self.enabled = false;
                self.run = None;
                self.fresh = false;
                if let Some(upstream) = &mut self.upstream {
                    let closed = upstream.call("agent_browser_close", json!({})).await?;
                    ensure!(
                        closed["isError"] != true,
                        "Browser close was not confirmed; owner control retained"
                    );
                }
                self.upstream.take();
                self.save_owner(None)?;
                self.interaction_pending = false;
            }
            _ => anyhow::bail!("Unknown browser operation"),
        }
        Ok(json!({"ok":true}))
    }
}

fn validate_input(tool: &str, args: &Value) -> Result<()> {
    let allowed: &[&str] = match tool {
        "agent_browser_mouse_move" => &["x", "y"],
        "agent_browser_mouse_down" | "agent_browser_mouse_up" => &["button"],
        "agent_browser_mouse_wheel" => &["dy"],
        "agent_browser_keyboard_insert_text" => &["text"],
        "agent_browser_press" => return browser_policy::validate(tool, args),
        "agent_browser_open" => return browser_policy::validate(tool, args),
        _ => anyhow::bail!("Owner input operation denied"),
    };
    let object = args.as_object().context("Invalid owner input")?;
    ensure!(
        object.len() == allowed.len() && allowed.iter().all(|k| object.contains_key(*k)),
        "Invalid owner input fields"
    );
    for (key, value) in object {
        match key.as_str() {
            "text" => ensure!(
                value.as_str().is_some_and(|s| s.len() <= 4096),
                "Text limit exceeded"
            ),
            "button" => ensure!(value == "left", "Invalid pointer button"),
            _ => ensure!(
                value.as_i64().is_some_and(|n| (-2000..=2000).contains(&n)),
                "Invalid pointer coordinate"
            ),
        }
    }
    Ok(())
}

pub async fn serve() -> Result<()> {
    unsafe {
        libc::umask(0o077);
    }
    let owner = if Path::new(OWNER).exists() {
        serde_json::from_slice(&std::fs::read(OWNER)?)?
    } else {
        None
    };
    let broker = Arc::new(Mutex::new(Broker {
        upstream: None,
        owner,
        enabled: false,
        run: None,
        fresh: false,
        poisoned: false,
        parked: false,
        interaction_pending: false,
        closed: false,
    }));
    let _ = std::fs::remove_file(super::browser_mcp::SOCKET);
    let listener = tokio::net::UnixListener::bind(super::browser_mcp::SOCKET)?;
    std::fs::set_permissions(
        super::browser_mcp::SOCKET,
        std::fs::Permissions::from_mode(0o660),
    )?;
    let permits = Arc::new(tokio::sync::Semaphore::new(8));
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    loop {
        let (stream, _) = tokio::select! {
            accepted = listener.accept() => accepted?,
            _ = terminate.recv() => {
                let mut locked = broker.lock().await;
                locked.enabled = false;
                if let Some(upstream) = &mut locked.upstream {
                    let _ = upstream.call("agent_browser_close", json!({})).await;
                }
                return Ok(());
            }
        };
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            continue;
        };
        let broker = broker.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let (read, mut write) = stream.into_split();
            let mut line = Vec::new();
            if !matches!(
                tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    BufReader::new(read)
                        .take(LIMIT + 1)
                        .read_until(b'\n', &mut line)
                )
                .await,
                Ok(Ok(_))
            ) || line.len() as u64 > LIMIT
                || !line.ends_with(b"\n")
            {
                return;
            }
            let Ok(request) = serde_json::from_slice::<Request>(&line) else {
                return;
            };
            let mut locked = broker.lock().await;
            let response = match locked.handle(request).await {
                Ok(value) => value,
                Err(error) => {
                    json!({"error":error.to_string(),"code":if locked.poisoned {"outcome_unknown"} else {"blocked"}})
                }
            };
            // Keep ownership until the preceding reply has left the broker.
            let _ = write.write_all(format!("{response}\n").as_bytes()).await;
            let _ = write.shutdown().await;
        });
    }
}

#[cfg(test)]
mod tests;
