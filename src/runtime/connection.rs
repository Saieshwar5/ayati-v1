//! Shared ACP connection/session initialization for ordinary and prepared chat execution.
use super::*;
use agent_client_protocol::{
    ByteStreams, Client,
    schema::{ProtocolVersion, v1::*},
};
use anyhow::Result;
use std::process::Stdio;
use tokio::sync::oneshot;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
impl GooseRuntime {
    pub(super) async fn connect(&self) -> Result<Process> {
        self.connect_with_preparation(false).await
    }
    pub(super) async fn connect_prepared(&self) -> Result<Process> {
        self.connect_with_preparation(true).await
    }
    async fn connect_with_preparation(&self, prepared: bool) -> Result<Process> {
        let model = &self.config.model;
        let connection_id = uuid::Uuid::new_v4().to_string();
        let mut child = {
            let mut command = if prepared {
                self.config.prepared_command().await?
            } else {
                self.config.command().await?
            };
            if let Some(control) = &self.config.control_tools {
                command
                    .env("AYATI_CONTROL_ADDRESS", &control.address)
                    .env("AYATI_CONTROL_TOKEN", &control.token);
            }
            let parent = std::process::id() as libc::pid_t;
            unsafe {
                command.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0
                        || libc::getppid() != parent
                    {
                        return Err(std::io::Error::other("Goose supervisor ended"));
                    }
                    Ok(())
                });
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
            let mut meta = self.config.session_meta()?;
            if prepared {
                meta.insert("ayatiPrepare".into(), serde_json::json!(1));
            }
            let mut settings = serde_json::json!({"provider":self.config.provider,"model":model,"proof_tool":self.config.proof_tool,"adapter":1});
            if self.config.control_tools.is_some() {
                settings["control_adapter"] = serde_json::json!(1);
            }
            let settings = settings.to_string();
            let saved = self.shared.store.runtime_session(&settings)?;
            ensure!(
                !prepared || saved.is_none(),
                "Prepared slots require a fresh empty session"
            );
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
                if prepared {
                    let duration = super::prepared::preparation_ack(created.meta.as_ref())?;
                    tracing::info!(
                        pid = resources.group,
                        generic_preparation_us = duration,
                        "Fresh harness completed generic preparation v1"
                    );
                }
                self.shared
                    .store
                    .save_runtime_session(&created.session_id.to_string(), &settings)?;
                created.session_id
            };
            Ok::<_, anyhow::Error>((connection, session))
        };
        match tokio::time::timeout(Duration::from_secs(30), setup).await {
            Ok(Ok((connection, session))) => Ok(Process {
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
}
