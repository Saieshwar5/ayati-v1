mod browser;
pub mod config;
mod connection;
mod events;
pub(crate) mod guidance;
pub(crate) mod prepared;
pub mod proof_tool;
pub mod provider;
mod tool_settings;

use crate::{
    agent::Event,
    storage::{Store, work::RunClaim},
};
use agent_client_protocol::{Agent, ConnectionTo, schema::v1::*};
use anyhow::{Context, Result, ensure};
use config::GooseConfig;
use events::RuntimeEvents;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{process::Child, sync::broadcast, task::JoinHandle};
use tokio_util::sync::CancellationToken;

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
        let control_tools = config.control_tools.is_some();
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
                control_tools,
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

    pub async fn prompt(
        &self,
        claim: &RunClaim,
        reply: &str,
        text: String,
        cancel: CancellationToken,
    ) -> Result<()> {
        self.prompt_inner(claim, reply, text, cancel, None).await
    }

    async fn prompt_inner(
        &self,
        claim: &RunClaim,
        reply: &str,
        text: String,
        cancel: CancellationToken,
        scoped: Option<prepared::ChatTurn<'_>>,
    ) -> Result<()> {
        let task = &claim.task_id;
        let profile = self.shared.store.profile_edit()?;
        let role_revision = profile.as_ref().map_or(0, |p| p.role_revision);
        let role = profile
            .as_ref()
            .map_or(self.config.role.as_str(), |p| p.role.as_str());
        let role = scoped.as_ref().map_or(role, |turn| turn.instructions);
        let identity = scoped
            .as_ref()
            .map_or(self.config.agent_id.as_str(), |turn| turn.agent);
        let timeout = scoped
            .as_ref()
            .map_or(self.config.run_timeout, |turn| turn.timeout);
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
            ensure!(scoped.is_none(), "Prepared harness is no longer ready");
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
        if scoped.is_some() || self.config.control_tools.is_some() {
            let _ = self.shared.events.send(crate::agent::Event {
                kind: "prompt".into(),
                task_id: claim.task_id.clone(),
                message_id: Some(reply.into()),
                text: None,
                offset: None,
            });
        }
        if let Some(turn) = &scoped {
            crate::identity::start_trace::point(turn.assignment, "executor", "acp_submit");
        }
        let prompt = p
            .connection
            .send_request(PromptRequest::new(
                p.session.clone(),
                vec![ContentBlock::Text(TextContent::new(
                    guidance::workspace_prompt(
                        identity,
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
            result = tokio::time::timeout(timeout, &mut prompt) => match result {
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
