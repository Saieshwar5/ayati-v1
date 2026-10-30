//! Process configuration, read once at startup. Each process validates its own
//! settings, reports every problem together and refuses other processes' secrets.
mod archive;
mod controller;
mod executor;
mod reader;

pub use archive::Archive;
pub use controller::{CloudWorkers, Controller, Execution, Library, Pool};
pub use executor::{Executor, Identity};
pub(crate) use reader::Reader;

use anyhow::Result;
use std::path::PathBuf;

/// Settings every command shares.
#[derive(Clone)]
pub struct Common {
    pub data_dir: PathBuf,
    pub port: u16,
}

impl Common {
    /// Data directory and port alone, for local health probes.
    pub fn from_env() -> Result<Self> {
        let mut env = Reader::process();
        let common = Self::read(&mut env);
        env.finish("health probe")?;
        Ok(common)
    }
    pub(crate) fn read(env: &mut Reader) -> Self {
        Self {
            data_dir: env
                .text("AYATI_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".ayati-local")),
            port: env.number("AYATI_PORT", 8765, 1..=65535),
        }
    }
}

/// Pinned Goose binary and the provider used when a profile names none.
#[derive(Clone)]
pub struct GooseSettings {
    pub binary: PathBuf,
    pub provider: String,
}

impl GooseSettings {
    pub(crate) fn read(env: &mut Reader) -> Self {
        let provider = env
            .text("AYATI_GOOSE_PROVIDER")
            .unwrap_or_else(|| "fireworks-ai".into());
        if !matches!(provider.as_str(), "openai" | "fireworks-ai") {
            env.problem("AYATI_GOOSE_PROVIDER must be openai or fireworks-ai");
        }
        Self {
            binary: env
                .text("AYATI_GOOSE_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".cache/goose/v1.54.0/goose")),
            provider,
        }
    }
}

/// Controller ↔ executor transport and its operator bootstrap key. The URL is
/// `http://` on a loopback IP for local development, otherwise `https://` to a
/// private host whose certificate is signed by `ca_file`.
#[derive(Clone)]
pub struct WorkerControl {
    pub url: String,
    /// Operator bootstrap key; an executor proving its AWS identity has none.
    pub key_file: PathBuf,
    /// The only certificate authority an executor trusts for an `https://` URL.
    pub ca_file: Option<PathBuf>,
}

impl WorkerControl {
    pub fn https(&self) -> bool {
        self.url.starts_with("https://")
    }
    #[cfg(test)]
    pub(crate) fn for_tests(url: &str, ca_file: Option<PathBuf>) -> Self {
        Self {
            url: url.into(),
            key_file: PathBuf::new(),
            ca_file,
        }
    }
}

impl WorkerControl {
    /// Both settings or neither; a half-configured listener is an error. An
    /// executor proving its AWS identity (`key == false`) must have no key file.
    pub(crate) fn read(env: &mut Reader, key: bool) -> Option<Self> {
        let key_file = env.text("AYATI_WORKER_CONTROL_KEY_FILE");
        if !key && key_file.is_some() {
            env.problem(
                "An executor proving its AWS identity uses no AYATI_WORKER_CONTROL_KEY_FILE",
            );
        }
        let key_file = if key { key_file } else { Some(String::new()) };
        match (env.text("AYATI_WORKER_CONTROL_URL"), key_file) {
            (Some(url), Some(key_file)) => {
                let key_file = PathBuf::from(key_file);
                if key && !key_file.is_absolute() {
                    env.problem("AYATI_WORKER_CONTROL_KEY_FILE must be an absolute path");
                }
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    env.problem("AYATI_WORKER_CONTROL_URL must be an http:// or https:// origin");
                }
                Some(Self {
                    url,
                    key_file,
                    ca_file: absolute(env, "AYATI_WORKER_CONTROL_CA"),
                })
            }
            (None, None) => None,
            _ => {
                env.problem(
                    "Set both AYATI_WORKER_CONTROL_URL and AYATI_WORKER_CONTROL_KEY_FILE, or neither",
                );
                None
            }
        }
    }
}

/// Log output, read before the other settings so their warnings are logged.
pub struct Logging {
    /// One JSON object per line for log collectors; plain text otherwise.
    pub json: bool,
    /// Image or build identifier included in the startup line.
    pub release: Option<String>,
}

impl Logging {
    pub fn from_env() -> Result<Self> {
        let env = Reader::process();
        let json = match env.text("AYATI_LOG_FORMAT").as_deref() {
            None | Some("text") => false,
            Some("json") => true,
            Some(_) => anyhow::bail!("AYATI_LOG_FORMAT must be text or json"),
        };
        Ok(Self {
            json,
            release: env.text("AYATI_RELEASE"),
        })
    }
}

pub(crate) fn absolute(env: &mut Reader, name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(env.text(name)?);
    if !path.is_absolute() {
        env.problem(format!("{name} must be an absolute path"));
    }
    Some(path)
}

/// The migration command only needs the administrator database URL.
pub fn migration_database_url() -> Result<String> {
    let mut env = Reader::process();
    let url = env.required("RELAY_DATABASE_URL");
    env.finish("workspace-db")?;
    Ok(url.unwrap_or_default())
}

/// Load the local development file for this command: `.env.executor` for
/// executors and their health probe, which must not see the controller's `.env`
/// secrets, and `.env` otherwise. Containers set `AYATI_DOTENV=0` so settings
/// come only from the task environment and its secrets.
pub fn load_dotenv(command: &str) {
    if std::env::var("AYATI_DOTENV").as_deref() == Ok("0") {
        return;
    }
    let file = if matches!(command, "executor" | "executor-health") {
        ".env.executor"
    } else {
        ".env"
    };
    dotenvy::from_filename(file).ok();
}
