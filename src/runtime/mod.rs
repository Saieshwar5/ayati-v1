mod browser;
pub mod config;
mod events;
pub(crate) mod guidance;
pub mod proof_tool;
pub mod provider;
mod tool_settings;

use crate::{
    agent::Event,
    storage::{Store, work::RunClaim},
};
use agent_client_protocol::{
    Agent, ByteStreams, Client, ConnectionTo,
    schema::{ProtocolVersion, v1::*},
};
use anyhow::{Context, Result, ensure};
use config::GooseConfig;
use events::RuntimeEvents;
use std::{
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
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
    browser: Option<browser::BrowserControl>,
    memory: Option<Arc<crate::memory::MemoryAccess>>,
    config: GooseConfig,
    vm: Option<Arc<crate::vm::VmManager>>,
    shared: Arc<RuntimeEvents>,
    process: tokio::sync::Mutex<Option<Process>>,
}

struct Turn {
    tools: Mutex<std::collections::HashMap<String, serde_json::Value>>,
    claim: RunClaim,
    connection_id: String,
    journal_failed: AtomicBool,
    reply: String,
    session: SessionId,
    cancel: CancellationToken,
    text: Mutex<String>,
}

struct Process {
    authority_revision: i64,
    role_revision: i64,
    connection_id: String,
    connection: ConnectionTo<Agent>,
    session: SessionId,
    resources: ProcessResources,
}

struct ProcessResources {
    group: i32,
    child: Child,
    driver: JoinHandle<()>,
    _memory_server: Option<crate::memory::HostMemoryServer>,
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
    pub async fn clear_working_context(&self) {
        self.process.lock().await.take();
    }
    pub fn platform(&self) -> Option<Arc<crate::platform::Platform>> {
        self.config.platform.clone()
    }
    pub fn identity(&self) -> &str {
        &self.config.agent_id
    }
    pub fn memory_enabled(&self) -> bool {
        self.shared.memory_enabled()
    }
    pub fn is_vm(&self) -> bool {
        self.vm.is_some()
    }

    pub fn request_vm_stop(&self) {
        if let Some(vm) = &self.vm {
            vm.request_stop();
        }
    }

    pub fn new(config: GooseConfig, store: Arc<Store>, events: broadcast::Sender<Event>) -> Self {
        let platform_enabled = config.platform.is_some();
        let panel_updates = config
            .platform
            .as_ref()
            .map(|p| (p.clone(), config.agent_id.clone()));
        let proof_tool = config.proof_tool;
        let browser_enabled = config.browser;
        let memory_enabled = config.memory;
        let memory = Some(Arc::new(
            crate::memory::MemoryAccess::new(store.clone())
                .with_enabled(config.memory)
                .with_platform(config.platform.clone(), &config.agent_id),
        ));
        let vm = config.vm_manifest.as_ref().map(|path| {
            Arc::new(
                crate::vm::VmManager::new(
                    path.clone(),
                    config.provider.clone(),
                    config.model.clone(),
                )
                .with_identity(config.agent_id.clone())
                .with_memory(memory.clone()),
            )
        });
        Self {
            browser: Some({
                browser::BrowserControl::load(config.root.parent().unwrap_or(&config.root))
                    .expect("Invalid persisted browser ownership")
            }),
            vm,
            memory,
            config,
            shared: Arc::new(RuntimeEvents {
                active: Mutex::new(None),
                store,
                events,
                proof_tool,
                browser: browser_enabled,
                memory: memory_enabled,
                platform: platform_enabled,
                panel_updates,
            }),
            process: tokio::sync::Mutex::new(None),
        }
    }

    async fn connect(&self) -> Result<Process> {
        let authority_revision = self
            .config
            .platform
            .as_ref()
            .map(|p| p.authority_revision(&self.config.agent_id))
            .transpose()?
            .unwrap_or(0);
        let mut effective = self.config.clone();
        effective.memory = self.shared.memory_enabled();
        effective.browser = self.shared.browser_enabled();
        effective.validate_browser()?;
        let profile = self.shared.store.profile_edit()?;
        let role_revision = profile.as_ref().map_or(0, |p| p.role_revision);
        let model = profile
            .as_ref()
            .and_then(|p| p.model.as_deref())
            .unwrap_or(&self.config.model);
        let connection_id = uuid::Uuid::new_v4().to_string();
        let (memory_server, memory_port) = if self.vm.is_none() {
            if let Some(memory) = &self.memory
                && (effective.memory || effective.platform.is_some())
            {
                let (server, port) = crate::memory::HostMemoryServer::start(memory.clone()).await?;
                (Some(server), port)
            } else {
                (None, 0)
            }
        } else {
            (None, 0)
        };
        let mut child = if let Some(vm) = &self.vm {
            vm.connect(&connection_id, model).await?
        } else {
            let mut config = effective.clone();
            config.model = model.into();
            let mut command = config.command().await?;
            if effective.memory || effective.platform.is_some() {
                command
                    .env("AYATI_MEMORY_ADDRESS", format!("127.0.0.1:{memory_port}"))
                    .env("AYATI_MEMORY_TOKEN", &connection_id);
            }
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .process_group(0)
                .spawn()
                .context("Could not start the pinned Goose runtime")?
        };
        let transport = ByteStreams::new(
            child.stdin.take().unwrap().compat_write(),
            child.stdout.take().unwrap().compat(),
        );
        let notification_id = connection_id.clone();
        let permission_id = connection_id.clone();
        let shared = self.shared.clone();
        let permissions = self.shared.clone();
        let (ready, receive) = oneshot::channel();
        let driver = tokio::spawn(async move {
            let _ = Client
                .builder()
                .name("ayati")
                .on_receive_notification(
                    async move |notification: SessionNotification, _cx| {
                        if shared.notification(&notification_id, notification).is_err() {
                            shared.journal_failed(&notification_id);
                            return Err(agent_client_protocol::Error::internal_error());
                        }
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .on_receive_request(
                    async move |request: RequestPermissionRequest, responder, _cx| {
                        responder.respond(permissions.permission(&permission_id, request).await)
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
            _memory_server: memory_server,
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
            let cwd = if self.vm.is_some() {
                std::path::PathBuf::from("/var/lib/ayati/workspace")
            } else {
                self.config.workspace.canonicalize()?
            };
            let meta = effective.session_meta()?;
            let mut settings = serde_json::json!({"provider":self.config.provider,"model":model,"proof_tool":self.config.proof_tool,"adapter":1});
            if role_revision > 0 {
                settings["role_revision"] = serde_json::json!(role_revision);
            }
            if effective.memory {
                settings["memory_adapter"] = serde_json::json!(1);
            }
            if effective.platform.is_some() {
                settings["workspace_adapter"] = serde_json::json!(1);
            }
            if authority_revision > 0 {
                settings["authority_revision"] = serde_json::json!(authority_revision);
            }
            if effective.browser {
                settings["browser_adapter"] = serde_json::json!(1);
            }
            if let Some(path) = &self.config.vm_manifest {
                settings["vm_manifest"] = serde_json::json!(path);
            }
            let settings = settings.to_string();
            let saved = if effective.platform.is_some() {
                self.shared.store.workspace_runtime_session(&settings)?
            } else {
                self.shared.store.runtime_session(&settings)?
            };
            let session = if let Some(saved) = saved {
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
                authority_revision,
                role_revision,
                connection_id,
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
        claim: &RunClaim,
        reply: &str,
        text: String,
        cancel: CancellationToken,
    ) -> Result<()> {
        let task = &claim.task_id;
        let profile = self.shared.store.profile_edit()?;
        let role_revision = profile.as_ref().map_or(0, |p| p.role_revision);
        let role = profile
            .as_ref()
            .map_or(self.config.role.as_str(), |p| p.role.as_str());
        let mut process = self.process.lock().await;
        let authority_revision = self
            .config
            .platform
            .as_ref()
            .map(|p| p.authority_revision(&self.config.agent_id))
            .transpose()?
            .unwrap_or(0);
        if process.as_mut().is_some_and(|p| {
            p.role_revision != role_revision
                || p.authority_revision != authority_revision
                || p.resources.driver.is_finished()
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
        ensure!(
            !cancel.is_cancelled() && self.shared.store.claim_active(claim)?,
            "Stopped before prompt submission"
        );
        let p = process.as_ref().unwrap();
        self.arm_browser(&p.connection_id).await?;
        if let Some(memory) = &self.memory
            && (self.memory_enabled() || self.config.platform.is_some())
        {
            memory.arm(&p.connection_id, claim, cancel.clone());
        }
        let turn = Arc::new(Turn {
            tools: Mutex::new(std::collections::HashMap::new()),
            claim: claim.clone(),
            connection_id: p.connection_id.clone(),
            journal_failed: AtomicBool::new(false),
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
                vec![ContentBlock::Text(TextContent::new(
                    guidance::workspace_prompt(
                        &self.config.agent_id,
                        role,
                        &text,
                        self.config.platform.as_deref(),
                    ),
                ))],
            ))
            .block_task();
        tokio::pin!(prompt);
        let mut result = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                let _ = p.connection.send_notification(CancelNotification::new(p.session.clone()));
                // A fresh connection will load only saved context on the next user message.
                let _ = tokio::time::timeout(Duration::from_secs(2), &mut prompt).await;
                Err(anyhow::anyhow!("Stopped; any already-completed actions cannot be undone"))
            },
            result = tokio::time::timeout(self.config.run_timeout, &mut prompt) => match result {
                Ok(Ok(response)) if response.stop_reason == StopReason::EndTurn => Ok(()),
                Ok(Ok(_)) => Err(anyhow::anyhow!("Goose ended before completion; send a message to continue")),
                Ok(Err(_)) => Err(anyhow::anyhow!("Goose request failed; its outcome may be uncertain. No automatic retry was made.")),
                Err(_) => Err(anyhow::anyhow!("Goose request timed out; no automatic retry was made")),
            }
        };
        if result.is_ok()
            && let Some(vm) = &self.vm
            && vm.model_failed().await
        {
            result = Err(anyhow::anyhow!(
                "Model service failed; no automatic prompt replay was made"
            ));
        }
        if turn.journal_failed.load(Ordering::SeqCst) {
            result = Err(anyhow::anyhow!(
                "Runtime action records could not be verified; no automatic retry was made"
            ));
        }
        if let Err(error) = self.disarm_browser().await {
            result = Err(error);
        }
        if let Some(memory) = &self.memory {
            memory.disarm();
        }
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
            .save_claim_reply(claim, reply, &turn.text.lock().unwrap(), status)?;
        if result.is_err() || self.shared.browser_enabled() || self.memory_enabled() {
            process.take();
        }
        result
    }

    pub async fn environment_status(&self) -> serde_json::Value {
        match &self.vm {
            Some(vm) => {
                let mut state = vm.status().await;
                state["execution"] = serde_json::json!("vm");
                state
            }
            None => serde_json::json!({"execution":"host"}),
        }
    }

    pub async fn start_vm(&self) -> Result<serde_json::Value> {
        let vm = self
            .vm
            .as_ref()
            .context("Select VM execution before starting an agent VM")?;
        vm.start().await?;
        Ok(self.environment_status().await)
    }

    pub async fn shutdown(&self) -> Result<()> {
        if let Some(mut process) = self.process.lock().await.take() {
            if let Some(id) = process.resources.child.id() {
                unsafe {
                    libc::kill(-(id as i32), libc::SIGKILL);
                }
            }
            let _ = process.resources.child.kill().await;
        }
        if let Some(vm) = &self.vm {
            vm.stop().await?;
        }
        Ok(())
    }
}
