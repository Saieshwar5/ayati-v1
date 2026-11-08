//! Agent computers: each agent may have one Linux computer for files, programs and
//! (later) a browser. The computer is permanent; its compute runs only while the agent
//! works. Providers run it; people never see which. Cuckoo's own types describe every
//! request and answer, so adding a provider is one file implementing [`Provider`] and
//! one line in [`adapter`]. See runtime/agent-computers.md.
mod e2b;
#[cfg(test)]
pub(crate) mod fake;
mod providers_file;
pub(in crate::identity) mod sleeper;
#[cfg(test)]
mod tests;

use anyhow::Result;
use std::time::Duration;

pub(crate) use providers_file::{ProvidersFile, Routed, SIZES};

/// The agent's home folder on every computer.
pub(crate) const HOME: &str = "/home/user";
/// Bytes of each of a command's stdout and stderr kept; longer output is cut.
pub(crate) const KEPT_BYTES: usize = 64 * 1024;
/// Characters of a command's output returned to the agent.
pub(crate) const SHOWN_CHARS: usize = 8_000;
/// The longest one command may run.
pub(crate) const LONGEST_COMMAND: Duration = Duration::from_secs(600);

/// Where and how to call a provider, from the providers file.
pub(crate) struct Call<'a> {
    pub api: &'a str,
    pub computers: &'a str,
    pub key: &'a str,
}

/// Which websites a computer may reach. Enforced by the provider on the computer's
/// network, so programs can't get around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Internet {
    None,
    Only(Vec<String>),
    Any,
}

/// A new computer.
pub(crate) struct Spec<'a> {
    /// The provider's name for Cuckoo's image at the chosen size.
    pub image: &'a str,
    /// The provider puts the computer to sleep by itself after this long, as a safety net.
    pub awake_seconds: u64,
    pub internet: &'a Internet,
    /// A label the provider keeps, for operators finding leftovers.
    pub label: &'a str,
}

/// An awake computer, ready for commands and files.
#[derive(Clone, Debug)]
pub(crate) struct Awake {
    pub id: String,
    pub token: String,
}

/// What a command did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ran {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    /// All output bytes, including what was cut.
    pub output_bytes: u64,
    pub timed_out: bool,
}

/// What every computer provider implements.
#[async_trait::async_trait]
pub(crate) trait Provider: Send + Sync {
    /// The name in the providers file and on records; never shown in workspaces.
    fn name(&self) -> &'static str;
    fn api_url(&self) -> &'static str;
    fn computer_url(&self) -> &'static str;
    async fn create(&self, call: &Call<'_>, spec: &Spec<'_>) -> Result<Awake>;
    /// Wakes a sleeping computer, or confirms an awake one, for `awake_seconds` more.
    async fn wake(&self, call: &Call<'_>, id: &str, awake_seconds: u64) -> Result<Awake>;
    /// Puts it to sleep keeping disk and memory; asleep costs storage only.
    async fn sleep(&self, call: &Call<'_>, id: &str) -> Result<()>;
    async fn delete(&self, call: &Call<'_>, id: &str) -> Result<()>;
    async fn set_internet(&self, call: &Call<'_>, id: &str, internet: &Internet) -> Result<()>;
    async fn run(
        &self,
        call: &Call<'_>,
        computer: &Awake,
        command: &str,
        cwd: &str,
        timeout: Duration,
    ) -> Result<Ran>;
    async fn write(
        &self,
        call: &Call<'_>,
        computer: &Awake,
        path: &str,
        bytes: Vec<u8>,
    ) -> Result<()>;
    async fn read(&self, call: &Call<'_>, computer: &Awake, path: &str) -> Result<Vec<u8>>;
}

/// Every provider this Cuckoo version supports, by its providers-file name.
pub(crate) fn adapter(name: &str) -> Option<&'static dyn Provider> {
    let found: &'static dyn Provider = match name {
        "e2b" => &e2b::E2b,
        _ => return None,
    };
    Some(found)
}

/// The controller's agent computers, when a providers file is configured.
#[derive(Clone)]
pub(crate) struct Computers {
    file: std::path::PathBuf,
    /// Awake computers' access, by provider id, with when it was last refreshed. A
    /// refresh also tells the provider to keep the computer awake a while longer.
    awake: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, (Awake, std::time::Instant)>>,
    >,
}

/// How often an awake computer's stay-awake time is renewed while in use.
pub(crate) const RENEW: Duration = Duration::from_secs(60);
impl Computers {
    /// The configured providers file, checked now so a broken one stops the
    /// controller at start instead of failing agents later.
    pub(crate) fn from(config: &crate::config::Controller) -> Result<Option<Self>> {
        let Some(file) = &config.computer_providers_file else {
            return Ok(None);
        };
        ProvidersFile::read(file)?;
        Ok(Some(Self::new(file.clone())))
    }
    pub(crate) fn new(file: std::path::PathBuf) -> Self {
        Self {
            file,
            awake: Default::default(),
        }
    }
    pub(crate) fn file(&self) -> Result<ProvidersFile> {
        ProvidersFile::read(&self.file)
    }
    /// The sizes people may choose, with what each costs per hour awake. Never the
    /// provider or what it costs Cuckoo. None when the file can't be read now.
    pub(crate) fn offered(&self) -> serde_json::Value {
        match self.file() {
            Ok(file) => serde_json::json!({
                "sleep_after_minutes": file.sleep_after_minutes,
                "sizes": file.sizes.iter().map(|s| serde_json::json!({
                    "size": s.size, "cpu": s.cpu, "memory_gb": s.memory_gb,
                    "disk_gb": s.disk_gb, "usd_per_hour": s.usd_per_hour,
                })).collect::<Vec<_>>(),
            }),
            Err(error) => {
                tracing::warn!("The computer providers file can't be read: {error}");
                serde_json::Value::Null
            }
        }
    }
    /// This computer's access, unless it needs renewing.
    pub(crate) fn fresh(&self, id: &str) -> Option<Awake> {
        let awake = self.awake.lock().ok()?;
        awake
            .get(id)
            .filter(|(_, at)| at.elapsed() < RENEW)
            .map(|(a, _)| a.clone())
    }
    pub(crate) fn remember(&self, awake: &Awake) {
        if let Ok(mut map) = self.awake.lock() {
            map.insert(awake.id.clone(), (awake.clone(), std::time::Instant::now()));
        }
    }
    pub(crate) fn forget(&self, id: &str) {
        if let Ok(mut map) = self.awake.lock() {
            map.remove(id);
        }
    }
}

/// Text from command output: valid UTF-8, at most `chars` characters, marked when cut.
pub(crate) fn clip(text: &str, chars: usize) -> String {
    match text.char_indices().nth(chars) {
        Some((at, _)) => format!("{}\n[… output shortened]", &text[..at]),
        None => text.to_owned(),
    }
}
