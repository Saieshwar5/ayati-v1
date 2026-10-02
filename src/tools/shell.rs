use super::{ShellInput, ToolResult};
use crate::files::Files;
use anyhow::{Context, Result, ensure};
use std::{
    path::{Component, Path},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const EXCERPT_BYTES: usize = 6000;
const LOG_BYTES: usize = 8 * 1024 * 1024;

pub struct Shell {
    files: Arc<Files>,
}
impl Shell {
    pub fn new(files: Arc<Files>) -> Self {
        Self { files }
    }

    pub async fn run(
        &self,
        task: &str,
        input: ShellInput,
        cancel: CancellationToken,
    ) -> Result<ToolResult> {
        ensure!(
            !input.command.trim().is_empty() && input.command.len() <= 16000,
            "command must contain 1–16000 bytes"
        );
        ensure!(
            (1..=120).contains(&input.timeout_seconds),
            "timeout_seconds must be 1–120"
        );
        let cwd = self.cwd(&input.cwd)?;
        let before = self.files.snapshot()?;
        let log_path = self.files.logs.join(Uuid::new_v4().to_string());
        let mut command = Command::new("bwrap");
        command.env_clear().args([
            "--die-with-parent",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
        ]);
        for path in ["/bin", "/lib", "/lib64"] {
            if let Ok(target) = std::fs::read_link(path) {
                command.arg("--symlink").arg(target).arg(path);
            } else if Path::new(path).is_dir() {
                command.args(["--ro-bind", path, path]);
            }
        }
        command
            .args([
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--tmpfs",
                "/tmp",
                "--clearenv",
                "--setenv",
                "PATH",
                "/usr/local/bin:/usr/bin:/bin",
                "--setenv",
                "HOME",
                "/workspace",
                "--setenv",
                "LANG",
                "C.UTF-8",
                "--setenv",
                "TMPDIR",
                "/tmp",
                "--bind",
            ])
            .arg(&self.files.workspace)
            .arg("/workspace")
            .arg("--ro-bind")
            .arg(&self.files.archives)
            .arg("/artifacts")
            .arg("--chdir")
            .arg(cwd)
            .args(["--", "/bin/sh", "-c"])
            .arg(&input.command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true);
        // Create the session before Bubblewrap forks. Its namespace init then
        // stays in this process group even if the command detaches itself.
        // SAFETY: setsid is async-signal-safe; the closure performs no allocation.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        ensure!(!cancel.is_cancelled(), "Stopped");
        let mut child = command.spawn().context("Could not start Bubblewrap; install bwrap and enable user namespaces. No unsandboxed fallback is used")?;
        let (tx, rx) = mpsc::channel(16);
        let stdout = tokio::spawn(read_pipe(
            child.stdout.take().context("No shell stdout")?,
            tx.clone(),
        ));
        let stderr = tokio::spawn(read_pipe(
            child.stderr.take().context("No shell stderr")?,
            tx,
        ));
        let log = tokio::spawn(capture(rx, log_path.clone()));
        let (status, exit_code) = tokio::select! {
            _ = cancel.cancelled() => { stop_sandbox(&mut child).await?; ("cancelled", None) }
            _ = tokio::time::sleep(Duration::from_secs(input.timeout_seconds)) => {
                stop_sandbox(&mut child).await?; ("timeout", None)
            }
            result = child.wait() => {
                let exit = result?;
                (if exit.success() { "success" } else { "error" }, exit.code())
            }
        };
        let _ = child.wait().await;
        stdout.await??;
        stderr.await??;
        let (output, truncated) = log.await??;
        let artifact = self
            .files
            .archive(task, "shell-output.txt", "log", &log_path)
            .await?;
        let _ = tokio::fs::remove_file(&log_path).await;
        let (artifacts, omitted_artifacts) = self.files.archive_changes(task, &before).await?;
        Ok(ToolResult {
            status: status.into(),
            exit_code,
            output,
            truncated,
            log_artifact_id: Some(artifact.id),
            log_path: Some(format!("/artifacts/{}", artifact.stored_name)),
            artifacts,
            omitted_artifacts,
        })
    }

    fn cwd(&self, cwd: &str) -> Result<String> {
        let relative = Path::new(cwd)
            .strip_prefix("/workspace")
            .context("cwd must be under /workspace")?;
        ensure!(
            relative
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
            "cwd cannot contain parent traversal"
        );
        let resolved = self
            .files
            .workspace
            .join(relative)
            .canonicalize()
            .context("cwd does not exist")?;
        ensure!(
            resolved.starts_with(&self.files.workspace) && resolved.is_dir(),
            "cwd must be a workspace directory"
        );
        Ok(format!(
            "/workspace/{}",
            resolved.strip_prefix(&self.files.workspace)?.display()
        ))
    }
}

async fn stop_sandbox(child: &mut tokio::process::Child) -> Result<()> {
    if let Some(pid) = child.id() {
        // The launcher has not been reaped, so its process-group ID cannot be
        // reused. Kill the namespace init too: --die-with-parent alone races
        // when cancellation arrives before sandbox setup completes.
        // SAFETY: kill only receives the dedicated group's ID and a signal.
        if unsafe { libc::kill(-(pid as i32), libc::SIGKILL) } == -1 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error).context("Could not stop sandbox process group");
            }
        }
    }
    child
        .wait()
        .await
        .context("Could not reap stopped sandbox")?;
    Ok(())
}

async fn read_pipe(mut pipe: impl AsyncRead + Unpin, tx: mpsc::Sender<Vec<u8>>) -> Result<()> {
    let mut buffer = vec![0; 8192];
    loop {
        let read = pipe.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        if tx.send(buffer[..read].to_vec()).await.is_err() {
            break;
        }
    }
    Ok(())
}

async fn capture(
    mut rx: mpsc::Receiver<Vec<u8>>,
    path: std::path::PathBuf,
) -> Result<(String, bool)> {
    let mut file = tokio::fs::File::create(path).await?;
    let mut excerpt = Vec::new();
    let mut total = 0usize;
    let mut logged = 0usize;
    while let Some(bytes) = rx.recv().await {
        total = total.saturating_add(bytes.len());
        let save = bytes.len().min(LOG_BYTES - logged);
        file.write_all(&bytes[..save]).await?;
        logged += save;
        let preview = bytes.len().min(EXCERPT_BYTES - excerpt.len());
        excerpt.extend_from_slice(&bytes[..preview]);
    }
    if total > LOG_BYTES {
        file.write_all(b"\n[Log stopped after 8 MB; remaining output omitted]\n")
            .await?;
    }
    file.flush().await?;
    let mut output = String::from_utf8_lossy(&excerpt).into_owned();
    if total > EXCERPT_BYTES {
        output.push_str("\n[Output excerpt truncated; see log artifact]");
    }
    Ok((output, total > EXCERPT_BYTES))
}
