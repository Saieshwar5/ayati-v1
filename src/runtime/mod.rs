pub mod config;
mod events;
pub mod proof_tool;

use crate::{agent::Event, storage::Store};
use agent_client_protocol::{
    Agent, ByteStreams, Client, ConnectionTo,
    schema::{ProtocolVersion, v1::*},
};
use anyhow::{Context, Result, ensure};
use config::GooseConfig;
use events::RuntimeEvents;
use std::{
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    process::Child,
    sync::{broadcast, oneshot},
    task::JoinHandle,
};
use tokio_util::{
    compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt},
    sync::CancellationToken,
};

pub struct GooseRuntime {
    config: GooseConfig,
    shared: Arc<RuntimeEvents>,
    process: tokio::sync::Mutex<Option<Process>>,
}

struct Turn {
    tools: Mutex<std::collections::HashMap<String, serde_json::Value>>,
    task: String,
    reply: String,
    session: SessionId,
    cancel: CancellationToken,
    text: Mutex<String>,
}

struct Process {
    connection: ConnectionTo<Agent>,
    session: SessionId,
    resources: ProcessResources,
}

struct ProcessResources {
    group: i32,
    child: Child,
    driver: JoinHandle<()>,
}

impl Drop for ProcessResources {
    fn drop(&mut self) {
        // Goose and its MCP subprocesses belong to our process group, never the host shell.
        unsafe {
            libc::kill(-self.group, libc::SIGKILL);
        }
        let _ = self.child.start_kill();
        self.driver.abort();
    }
}

impl GooseRuntime {
    pub fn new(config: GooseConfig, store: Arc<Store>, events: broadcast::Sender<Event>) -> Self {
        let proof_tool = config.proof_tool;
        Self {
            config,
            shared: Arc::new(RuntimeEvents {
                active: Mutex::new(None),
                store,
                events,
                proof_tool,
            }),
            process: tokio::sync::Mutex::new(None),
        }
    }

    async fn connect(&self) -> Result<Process> {
        let mut command = self.config.command().await?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .process_group(0)
            .spawn()
            .context("Could not start the pinned Goose runtime")?;
        let transport = ByteStreams::new(
            child.stdin.take().unwrap().compat_write(),
            child.stdout.take().unwrap().compat(),
        );
        let shared = self.shared.clone();
        let permissions = self.shared.clone();
        let (ready, receive) = oneshot::channel();
        let driver = tokio::spawn(async move {
            let _ = Client
                .builder()
                .name("ayati")
                .on_receive_notification(
                    async move |notification: SessionNotification, _cx| {
                        shared
                            .notification(notification)
                            .map_err(|_| agent_client_protocol::Error::internal_error())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .on_receive_request(
                    async move |request: RequestPermissionRequest, responder, _cx| {
                        responder.respond(permissions.permission(request).await)
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .connect_with(transport, async move |cx: ConnectionTo<Agent>| {
                    let _ = ready.send(cx);
                    std::future::pending::<()>().await;
                    Ok(())
                })
                .await;
        });
        let resources = ProcessResources {
            group: child.id().unwrap() as i32,
            child,
            driver,
        };
        let setup = async {
            let connection = receive
                .await
                .context("Goose connection closed during startup")?;
            let init = connection
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await
                .map_err(|_| anyhow::anyhow!("Goose initialization failed"))?;
            ensure!(
                init.protocol_version == ProtocolVersion::V1
                    && init.agent_capabilities.load_session,
                "Pinned Goose lacks the required ACP v1 session-loading capability"
            );
            let cwd = self.config.workspace.canonicalize()?;
            let meta = self.config.session_meta()?;
            let settings = serde_json::json!({"provider":self.config.provider,"model":self.config.model,"proof_tool":self.config.proof_tool,"adapter":1}).to_string();
            let session = if let Some(saved) = self.shared.store.runtime_session(&settings)? {
                let id = SessionId::from(saved);
                connection
                    .send_request(LoadSessionRequest::new(id.clone(), cwd).meta(meta))
                    .block_task()
                    .await
                    .map_err(|_| {
                        anyhow::anyhow!(
                            "Goose could not load the saved session; no prompt was replayed"
                        )
                    })?;
                id
            } else {
                let created = connection
                    .send_request(NewSessionRequest::new(cwd).meta(meta))
                    .block_task()
                    .await
                    .map_err(|_| {
                        anyhow::anyhow!("Goose session setup failed; check provider configuration")
                    })?;
                self.shared
                    .store
                    .save_runtime_session(&created.session_id.to_string(), &settings)?;
                created.session_id
            };
            Ok::<_, anyhow::Error>((connection, session))
        };
        match tokio::time::timeout(Duration::from_secs(30), setup).await {
            Ok(Ok((connection, session))) => Ok(Process {
                connection,
                session,
                resources,
            }),
            error => {
                drop(resources);
                match error {
                    Ok(Err(e)) => Err(e),
                    _ => anyhow::bail!("Goose startup timed out"),
                }
            }
        }
    }

    pub async fn prompt(
        &self,
        task: &str,
        reply: &str,
        text: String,
        cancel: CancellationToken,
    ) -> Result<()> {
        let mut process = self.process.lock().await;
        if process.as_mut().is_some_and(|p| {
            p.resources.driver.is_finished()
                || p.resources.child.try_wait().ok().flatten().is_some()
        }) {
            process.take();
        }
        if process.is_none() {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => anyhow::bail!("Stopped before Goose started"),
                result = self.connect() => *process = Some(result?),
            }
        }
        ensure!(!cancel.is_cancelled(), "Stopped before prompt submission");
        let p = process.as_ref().unwrap();
        let turn = Arc::new(Turn {
            tools: Mutex::new(std::collections::HashMap::new()),
            task: task.into(),
            reply: reply.into(),
            session: p.session.clone(),
            cancel: cancel.clone(),
            text: Mutex::new(String::new()),
        });
        *self.shared.active.lock().unwrap() = Some(turn.clone());
        let prompt = p
            .connection
            .send_request(PromptRequest::new(
                p.session.clone(),
                vec![ContentBlock::Text(TextContent::new(text))],
            ))
            .block_task();
        tokio::pin!(prompt);
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                let _ = p.connection.send_notification(CancelNotification::new(p.session.clone()));
                // A fresh connection will load only saved context on the next user message.
                let _ = tokio::time::timeout(Duration::from_secs(2), &mut prompt).await;
                Err(anyhow::anyhow!("Stopped; any already-completed actions cannot be undone"))
            },
            result = tokio::time::timeout(Duration::from_secs(180), &mut prompt) => match result {
                Ok(Ok(response)) if response.stop_reason == StopReason::EndTurn => Ok(()),
                Ok(Ok(_)) => Err(anyhow::anyhow!("Goose ended before completion; send a message to continue")),
                Ok(Err(_)) => Err(anyhow::anyhow!("Goose request failed; its outcome may be uncertain. No automatic retry was made.")),
                Err(_) => Err(anyhow::anyhow!("Goose request timed out; no automatic retry was made")),
            }
        };
        *self.shared.active.lock().unwrap() = None;
        self.shared.store.cancel_permissions(task)?;
        let status = if cancel.is_cancelled() {
            "stopped"
        } else if result.is_ok() {
            "complete"
        } else {
            "failed"
        };
        self.shared
            .store
            .save_reply(reply, &turn.text.lock().unwrap(), status)?;
        if result.is_err() {
            process.take();
        }
        result
    }

    pub async fn shutdown(&self) {
        if let Some(mut process) = self.process.lock().await.take() {
            if let Some(id) = process.resources.child.id() {
                unsafe {
                    libc::kill(-(id as i32), libc::SIGKILL);
                }
            }
            let _ = process.resources.child.kill().await;
        }
    }
}
