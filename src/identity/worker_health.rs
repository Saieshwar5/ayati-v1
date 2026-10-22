//! Optional process-local readiness signal for container supervisors.
use super::{postgres::PgStore, store::now};
use anyhow::{Context, Result, ensure};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) struct Heartbeat {
    path: PathBuf,
    _lock: std::fs::File,
    checked: Option<Instant>,
}
impl Heartbeat {
    pub fn from_env() -> Result<Option<Self>> {
        std::env::var_os("AYATI_WORKER_HEALTH_FILE")
            .map(|path| Self::open(path.into()))
            .transpose()
    }
    fn open(path: PathBuf) -> Result<Self> {
        ensure!(
            path.is_absolute(),
            "AYATI_WORKER_HEALTH_FILE must be absolute"
        );
        let lock = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))?;
        lock.try_lock()
            .context("Another worker uses this health file")?;
        let _ = std::fs::remove_file(&path);
        Ok(Self {
            path,
            _lock: lock,
            checked: None,
        })
    }
    pub async fn tick(&mut self, pg: &PgStore) {
        if self
            .checked
            .is_some_and(|at| at.elapsed() < Duration::from_secs(5))
        {
            return;
        }
        self.checked = Some(Instant::now());
        if matches!(
            tokio::time::timeout(Duration::from_secs(3), pg.ping()).await,
            Ok(Ok(()))
        ) && self.publish().is_ok()
        {
            return;
        }
        let _ = std::fs::remove_file(&self.path);
        tracing::warn!("Worker readiness unavailable");
    }
    fn publish(&self) -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let outcome = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            write!(file, "{} {}", std::process::id(), now())?;
            std::fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        let _ = std::fs::remove_file(temporary);
        outcome
    }
}
impl Drop for Heartbeat {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
pub fn check() -> Result<()> {
    let path =
        std::env::var_os("AYATI_WORKER_HEALTH_FILE").context("Set AYATI_WORKER_HEALTH_FILE")?;
    validate(&std::fs::read_to_string(path).context("Worker is not ready")?)
}
fn validate(value: &str) -> Result<()> {
    let mut fields = value.split_whitespace();
    let pid: i32 = fields.next().context("Missing worker PID")?.parse()?;
    let at: i64 = fields.next().context("Missing worker heartbeat")?.parse()?;
    ensure!(
        pid > 0 && fields.next().is_none() && (0..=20).contains(&(now() - at)),
        "Worker heartbeat is stale"
    );
    // Same-UID process check. The supervisor must run inside this worker's PID namespace.
    ensure!(
        unsafe { libc::kill(pid, 0) } == 0,
        "Worker process is not running"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn heartbeat_requires_live_process_fresh_timestamp_and_exclusive_writer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worker.health");
        let heartbeat = Heartbeat::open(path.clone()).unwrap();
        assert!(Heartbeat::open(path.clone()).is_err());
        heartbeat.publish().unwrap();
        assert!(validate(&std::fs::read_to_string(&path).unwrap()).is_ok());
        for invalid in [
            format!("{} {}", std::process::id(), now() - 21),
            format!("{} {}", std::process::id(), now() + 10),
            format!("2147483647 {}", now()),
            format!("0 {}", now()),
            "bad".into(),
        ] {
            assert!(validate(&invalid).is_err());
        }
        drop(heartbeat);
        assert!(!path.exists());
    }
}
