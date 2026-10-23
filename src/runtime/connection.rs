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
}
