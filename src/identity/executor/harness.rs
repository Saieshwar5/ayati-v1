//! One prepared Goose process/session per use. Never reset or reuse a tenant transcript.
use super::model::Profile;
use crate::{
    identity::chat::execution::{self, Prepared},
    runtime::{GooseRuntime, config::GooseConfig},
    storage::Store,
};
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

pub(super) struct Harness {
    runtime: Arc<GooseRuntime>,
    store: Arc<Store>,
    root: PathBuf,
    profile: Profile,
    pub pid: u32,
}
impl Harness {
    pub async fn prepare(root: &Path, profile: &Profile) -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(root)?;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
        let mut config = GooseConfig::from_env(root, &profile.model);
        config.provider = profile.provider.clone();
        config.agent_id = "prepared-chat".into();
        config.vm_manifest = None;
        config.browser = false;
        config.memory = false;
        config.platform = None;
        config.proof_tool = false;
        ensure!(
            config.configured(),
            "Prepared executor requires pinned Goose and a configured provider"
        );
        let store = Arc::new(Store::open(&root.join("chat.sqlite"))?);
        let runtime = Arc::new(GooseRuntime::new(
            config,
            store.clone(),
            tokio::sync::broadcast::channel(256).0,
        ));
        let pid = runtime.prepare_chat().await?;
        Ok(Self {
            runtime,
            store,
            root: root.into(),
            profile: profile.clone(),
            pid,
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
    pub async fn run(self, context: Prepared, cancel: CancellationToken) -> Result<String> {
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
            let task = self
                .store
                .accept_input("Controller-scoped shared request", Some(&context.reply))?;
            let claim = self
                .store
                .claim_next(&uuid::Uuid::new_v4().to_string())?
                .context("Prepared claim unavailable")?;
            ensure!(
                claim.task_id == task.id,
                "Prepared session must contain only its assigned task"
            );
            let reply = self.store.add_claim_reply(&claim)?;
            let result = self
                .runtime
                .prompt_chat(
                    &claim,
                    &reply,
                    execution::prompt(&context),
                    cancel,
                    crate::runtime::prepared::ChatTurn {
                        agent: &context.agent,
                        instructions: &context.instructions,
                        timeout: std::time::Duration::from_secs(context.run_seconds),
                    },
                )
                .await;
            self.store.finish_work(
                &claim,
                if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                },
                "",
            )?;
            result?;
            let text = self
                .store
                .messages()?
                .into_iter()
                .find(|m| m.id == reply && m.state == "complete")
                .context("Prepared session produced no completed reply")?
                .content;
            Ok(format!(
                "{text}{}",
                crate::identity::library::context::citations(&context.workspace, &context.sources)
            ))
        }
        .await;
        self.close().await;
        result
    }
}
