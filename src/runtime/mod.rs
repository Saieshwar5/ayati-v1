pub mod config;
mod connection;
mod events;
pub(crate) mod guidance;
pub(crate) mod prepared;
pub mod proof_tool;
pub mod provider;

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
    config: GooseConfig,
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
    pub fn identity(&self) -> &str {
        &self.config.agent_id
    }
    pub fn new(config: GooseConfig, store: Arc<Store>, events: broadcast::Sender<Event>) -> Self {
        let control_tools = config.control_tools.is_some();
        let proof_tool = config.proof_tool;
        Self {
            config,
            shared: Arc::new(RuntimeEvents {
                control_tools,
                active: Mutex::new(None),
                store,
                events,
                proof_tool,
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
        if process.as_mut().is_some_and(|p| {
            p.role_revision != role_revision
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
        if let Some(assignment) = scoped
            .as_ref()
            .map(|turn| turn.assignment)
            .or(self.config.trace_assignment.as_deref())
        {
            crate::identity::start_trace::point(assignment, "executor", "acp_submit");
        }
        let prompt = p
            .connection
            .send_request(PromptRequest::new(
                p.session.clone(),
                vec![ContentBlock::Text(TextContent::new(
                    if scoped.is_some() || self.config.managed_chat {
                        guidance::managed_prompt(role, &text)
                    } else {
                        guidance::prompt(identity, role, &text)
                    },
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
        if turn.journal_failed.load(Ordering::SeqCst) {
            result = Err(anyhow::anyhow!(
                "Runtime action records could not be verified; no automatic retry was made"
            ));
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
        if result.is_err() {
            process.take();
        }
        result
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
        Ok(())
    }
}
