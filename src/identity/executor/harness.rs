//! One prepared Goose process/session per use. Never reset or reuse a tenant transcript.
use super::model::Profile;
use crate::{
    identity::chat::execution::{self, Prepared},
    journal::Journal,
    runtime::{GooseRuntime, config::GooseConfig},
};
use anyhow::{Result, ensure};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

pub(super) struct Harness {
    runtime: Arc<GooseRuntime>,
    store: Arc<Journal>,
    root: PathBuf,
    profile: Profile,
    pub pid: u32,
    events: tokio::sync::broadcast::Sender<crate::runtime::Event>,
}
impl Harness {
    pub async fn prepare(root: &Path, profile: &Profile) -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(root)?;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
        // Stop dotenv discovery at this fresh, owned scratch boundary.
        std::fs::write(root.join(".env"), "")?;
        let mut config = GooseConfig::from_env(root, &profile.model);
        config.binary = std::env::var_os("AYATI_PREPARED_GOOSE_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".cache/goose/v1.54.0-ayati.1/goose"));
        config.provider = profile.provider.clone();
        config.agent_id = "prepared-chat".into();
        config.proof_tool = false;
        ensure!(
            config.configured(),
            "Prepared executor requires pinned Goose and a configured provider"
        );
        let store = Arc::new(Journal::open(&root.join("chat.sqlite"))?);
        let events = tokio::sync::broadcast::channel(256).0;
        let runtime = Arc::new(GooseRuntime::new(config, store.clone(), events.clone()));
        let pid = runtime.prepare_chat().await?;
        Ok(Self {
            runtime,
            store,
            root: root.into(),
            profile: profile.clone(),
            pid,
            events,
        })
    }
    pub async fn ready(&self) -> bool {
        self.runtime.chat_ready().await
    }
    pub fn shutdown_handle(&self) -> Arc<GooseRuntime> {
        self.runtime.clone()
    }
    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }
    pub async fn close(self) {
        let _ = self.runtime.shutdown().await;
        execution::cleanup(&self.root).await;
    }
    pub async fn run(
        self,
        context: Prepared,
        cancel: CancellationToken,
        mut output: super::output::Output,
    ) -> Result<String> {
        output.trace.mark("controller_roundtrip");
        let result = async {
            ensure!(context.version == 1, "Unsupported prepared context version");
            ensure!(
                context.model == self.profile.model
                    && context
                        .provider
                        .as_deref()
                        .unwrap_or(&self.profile.provider)
                        == self.profile.provider,
                "Prepared harness profile no longer matches the assignment"
            );
            ensure!(
                !cancel.is_cancelled() && self.ready().await,
                "Prepared harness unavailable"
            );
            // Existing prepared slots are tool-free. A tool job gets a fresh scoped
            // harness, with its cold startup explicitly included in timing.
            let broker = if context.control_tools {
                self.runtime.shutdown().await?;
                // The empty prepared session has a different tool fingerprint.
                // Discard that session pointer instead of loading it with new tools.
                self.store.clear_runtime_session()?;
                output.trace.mark("tool_harness_cold_start");
                Some(
                    crate::identity::control::broker::Broker::start(
                        crate::identity::control::broker::Access::Remote(
                            output.client.clone(),
                            output.assignment.clone(),
                        ),
                    )
                    .await?,
                )
            } else {
                None
            };
            let tool_runtime = if let Some(broker) = &broker {
                let mut config =
                    execution::runtime_config(&context, &self.root, Some(broker.endpoint.clone()))?;
                config.trace_assignment = Some(output.assignment.scope.assignment.clone());
                Some(Arc::new(GooseRuntime::new(
                    config,
                    self.store.clone(),
                    self.events.clone(),
                )))
            } else {
                None
            };
            let (claim, reply) = crate::runtime::turn::begin(&self.store, &context.reply)?;
            output.trace.mark("local_setup");
            let assignment = output.assignment.scope.assignment.clone();
            crate::identity::start_trace::point(&assignment, "executor", "local_setup_done");
            let done = CancellationToken::new();
            let finished = done.clone().drop_guard();
            let sender = tokio::spawn(output.forward(
                self.events.subscribe(),
                reply.clone(),
                done.clone(),
                cancel.clone(),
            ));
            let result = if let Some(runtime) = &tool_runtime {
                runtime
                    .prompt(&claim, &reply, execution::prompt(&context), cancel.clone())
                    .await
            } else {
                self.runtime
                    .prompt_chat(
                        &claim,
                        &reply,
                        execution::prompt(&context),
                        cancel.clone(),
                        crate::runtime::prepared::ChatTurn {
                            assignment: &assignment,
                            instructions: &context.instructions,
                            timeout: std::time::Duration::from_secs(context.run_seconds),
                        },
                    )
                    .await
            };
            drop(finished);
            sender.await??;
            if let Some(runtime) = tool_runtime {
                runtime.shutdown().await?;
            }
            let text = crate::runtime::turn::finish(&self.store, &claim, &reply, result)?;
            Ok(format!(
                "{text}{}",
                crate::identity::library::context::citations(&context.workspace, &context.sources)
            ))
        }
        .await;
        if let Err(error) = &result {
            let message = error.to_string();
            let stage = if message.starts_with("Expected pinned Goose") {
                "pinned_version"
            } else if message.starts_with("Goose session setup failed") {
                "session_setup"
            } else if message.starts_with("Goose initialization failed") {
                "acp_initialization"
            } else if message.starts_with("Goose") {
                "goose_execution"
            } else if message.starts_with("Controller") {
                "controller"
            } else if message.starts_with("Stopped") {
                "stopped"
            } else {
                "assignment_or_output"
            };
            tracing::warn!(
                stage,
                "Shared harness did not complete; request contents are omitted"
            );
        }
        self.close().await;
        result
    }
}
