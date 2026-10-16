use super::{Workspace, registry::AgentProfile};
use anyhow::{Context, Result, ensure};
use std::{
    os::unix::fs::OpenOptionsExt,
    process::Stdio,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

// The preparation helper starts QEMU and SSH children. Cancellation must reap
// the entire dedicated process group, including when its parent exits first.
struct ProcessGroup(i32);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}
impl Workspace {
    pub(super) fn spawn_preparation(
        self: &Arc<Self>,
        mut profile: AgentProfile,
    ) -> tokio::task::JoinHandle<()> {
        let workspace = self.clone();
        tokio::spawn(async move {
            let result = workspace.prepare(&profile).await;
            let _operation = workspace.operation.lock().unwrap();
            let result = result.and_then(|()| {
                ensure!(
                    !workspace.closing.load(Ordering::SeqCst),
                    "Setup interrupted by shutdown"
                );
                workspace.activate(&profile)
            });
            match result {
                Ok(()) => {
                    profile.state = "ready".into();
                    profile.detail = "Ready for your first message.".into();
                }
                Err(error) => {
                    tracing::warn!(agent_id=%profile.id, %error, "Agent setup failed; private setup log preserved");
                    profile.state = "failed".into();
                    profile.detail = "Setup failed or was interrupted. Check the private setup log, then retry with a fresh environment.".into();
                }
            }
            if let Err(error) = workspace.registry.save(&profile) {
                // Never expose an agent whose ready state was not persisted.
                if let Some(agent) = workspace.agents.lock().unwrap().remove(&profile.id) {
                    agent.begin_shutdown();
                }
                tracing::error!(%error, "Could not save agent setup result");
            }
        })
    }
    async fn prepare(&self, profile: &AgentProfile) -> Result<()> {
        let directory = self.vm_path(profile).parent().unwrap().to_owned();
        let logs = self.data.join("setup-logs");
        std::fs::create_dir_all(&logs)?;
        let log = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(logs.join(format!("{}.log", profile.attempt)))?;
        let mut command = tokio::process::Command::new("/usr/bin/python3");
        command
            .arg("tools/prepare-local-vm.py")
            .arg("--directory")
            .arg(&directory)
            .arg("--agent-id")
            .arg(&profile.id)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C.UTF-8")
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .process_group(0)
            .kill_on_drop(true);
        if profile.browser {
            command.arg("--browser");
        }
        for key in ["AYATI_QEMU_BIN", "AYATI_OVMF_DIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        // A daemon crash signals the helper so its finally block stops QEMU.
        let parent = unsafe { libc::getpid() };
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::other("Setup parent exited"));
                }
                Ok(())
            });
        }
        let mut child = command
            .spawn()
            .context("Could not start the local VM preparation helper")?;
        let _group = ProcessGroup(child.id().context("Missing setup process")? as i32);
        tokio::select! {
            biased;
            _ = self.cancelled.cancelled() => { child.kill().await?; anyhow::bail!("Setup cancelled"); }
            result = tokio::time::timeout(Duration::from_secs(300), child.wait()) => {
                ensure!(result.context("Agent setup timed out")??.success(), "VM preparation failed; inspect the setup log");
            }
        }
        Ok(())
    }
}
