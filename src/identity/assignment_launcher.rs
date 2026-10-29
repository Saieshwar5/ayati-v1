//! Local controller adapter for durable launch intents. AWS task launching is separate work.
use super::{
    postgres::PgStore,
    worker_config::{self, Mode},
};
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

/// Concrete local provider boundary: start an identified assignment and observe its child.
pub(super) struct LocalProvider {
    root: PathBuf,
    goose: PathBuf,
}
impl LocalProvider {
    pub(super) fn new(data: &Path) -> Result<Self> {
        Ok(Self {
            root: data.join("assignment-launches"),
            goose: crate::runtime::config::GooseConfig::from_env(data, "unused")
                .binary
                .canonicalize()
                .context("Install/configure pinned Goose before launching assignments")?,
        })
    }
    pub(super) async fn launch(
        &self,
        assignment: &str,
        grant: Option<&super::worker_control::model::Launch>,
    ) -> Result<Child> {
        let root = self.root.join(assignment);
        std::fs::create_dir_all(&root)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        // Do not discover an operator's parent .env in this isolated worker directory.
        std::fs::write(root.join(".env"), b"")?;
        let root = root.canonicalize()?;
        let mut command = Command::new(std::env::current_exe()?);
        if grant.is_some() {
            command.args(["worker", "--controller-assignment"]);
        } else {
            command.args(["worker", "--assignment", assignment]);
        }
        command
            .current_dir(&root)
            .env_clear()
            .env("AYATI_DATA_DIR", &root)
            .env("AYATI_CHAT_EXECUTION", "assigned")
            .env("AYATI_GOOSE_BIN", &self.goose)
            .stdin(if grant.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        for name in [
            "PATH",
            "LANG",
            "RUST_LOG",
            "AYATI_GOOSE_PROVIDER",
            "AYATI_MODEL",
            "OPENAI_API_KEY",
            "FIREWORKS_API_KEY",
            "OPENAI_HOST",
            "OPENAI_BASE_PATH",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        if grant.is_none()
            && let Some(value) = std::env::var_os("RELAY_DATABASE_URL")
        {
            command.env("RELAY_DATABASE_URL", value);
        }
        // Local launcher loss cannot leave an untracked worker/Goose process tree.
        let parent = std::process::id() as libc::pid_t;
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0
                    || libc::getppid() != parent
                {
                    return Err(std::io::Error::other("Local launcher ended"));
                }
                Ok(())
            });
        }
        let mut child = command
            .spawn()
            .context("Could not start local assignment worker")?;
        if let Some(grant) = grant {
            use tokio::io::AsyncWriteExt;
            let mut input = child.stdin.take().context("Missing private launch pipe")?;
            input.write_all(&serde_json::to_vec(grant)?).await?;
            input.shutdown().await?;
        }
        Ok(child)
    }
}

pub async fn serve(data: &Path) -> Result<()> {
    ensure!(
        Mode::from_env(true)? == Mode::Assigned,
        "The launcher requires AYATI_CHAT_EXECUTION=assigned"
    );
    if std::env::var_os("AYATI_WORKER_CONTROL_URL").is_some()
        || std::env::var_os("AYATI_WORKER_CONTROL_KEY_FILE").is_some()
    {
        return super::controlled_launcher::serve(data).await;
    }
    let capacity = worker_config::seconds("AYATI_ASSIGNMENT_CAPACITY", 2, 1, 16)? as usize;
    let lease = worker_config::seconds("AYATI_ASSIGNMENT_LEASE_SECONDS", 120, 6, 300)?;
    let pg = Arc::new(
        PgStore::connect(
            &std::env::var("RELAY_DATABASE_URL")
                .context("The launcher requires RELAY_DATABASE_URL")?,
        )
        .await?,
    );
    pg.verify_schema().await?;
    let provider = LocalProvider::new(data)?;
    let stopping = CancellationToken::new();
    let signal = stopping.clone();
    let signal_task = tokio::spawn(async move {
        super::worker::shutdown_signal().await;
        signal.cancel();
    });
    let mut children = JoinSet::new();
    tracing::info!(capacity, "Local assignment launcher ready");
    while !stopping.is_cancelled() {
        while children.len() < capacity && !stopping.is_cancelled() {
            let assignment = match pg.reserve_assignment(lease, capacity).await {
                Ok(Some(id)) => id,
                Ok(None) => break,
                Err(_) => {
                    tracing::warn!("Assignment admission unavailable; will retry");
                    break;
                }
            };
            match provider.launch(&assignment, None).await {
                Ok(mut child) => {
                    let resource = format!("local:{}", child.id().context("Worker PID missing")?);
                    // Worker registration can win this race; recording the same identity is idempotent.
                    if !matches!(
                        pg.record_assignment_resource(&assignment, &resource).await,
                        Ok(true)
                    ) {
                        let _ = child.kill().await;
                        let _ = pg
                            .release_assignment(&assignment, "launch_failed", Some(&resource))
                            .await;
                        continue;
                    }
                    let db = pg.clone();
                    let shutdown = stopping.clone();
                    children.spawn(async move {
                        let status = tokio::select! {
                            result = child.wait() => result,
                            _ = shutdown.cancelled() => {
                                if let Some(pid)=child.id() { unsafe { libc::kill(pid as libc::pid_t,libc::SIGTERM); } }
                                match tokio::time::timeout(Duration::from_secs(15),child.wait()).await {
                                    Ok(status) => status,
                                    Err(_) => { let _=child.kill().await; child.wait().await }
                                }
                            }
                        };
                        // The process is stopped. This cannot release a newer attempt's claim.
                        let reason = if shutdown.is_cancelled() { "interrupted" } else { "lost" };
                        if db.release_assignment(&assignment,reason,Some(&resource)).await.is_err() {
                            tracing::warn!(assignment=%assignment,"Worker exit reconciliation deferred");
                        }
                        tracing::info!(assignment=%assignment,success=status.is_ok_and(|s|s.success()),"Local worker stopped");
                    });
                }
                Err(_) => {
                    let _ = pg
                        .release_assignment(&assignment, "launch_failed", None)
                        .await;
                }
            }
        }
        tokio::select! {
            _ = stopping.cancelled() => {},
            _ = children.join_next(), if !children.is_empty() => {},
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
    }
    while children.join_next().await.is_some() {}
    signal_task.abort();
    Ok(())
}
