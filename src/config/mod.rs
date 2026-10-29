//! Process configuration, read once at startup. Each process validates its own
//! settings, reports every problem together and refuses other processes' secrets.
mod controller;
mod executor;
mod reader;

pub use controller::{CloudWorkers, Controller, Execution, Library, Pool};
pub use executor::Executor;
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

/// Loopback controller ↔ executor transport and its operator bootstrap key.
#[derive(Clone)]
pub struct WorkerControl {
    pub url: String,
    pub key_file: PathBuf,
}

impl WorkerControl {
    /// Both settings or neither; a half-configured listener is an error.
    pub(crate) fn read(env: &mut Reader) -> Option<Self> {
        match (
            env.text("AYATI_WORKER_CONTROL_URL"),
            env.text("AYATI_WORKER_CONTROL_KEY_FILE"),
        ) {
            (Some(url), Some(key_file)) => {
                let key_file = PathBuf::from(key_file);
                if !key_file.is_absolute() {
                    env.problem("AYATI_WORKER_CONTROL_KEY_FILE must be an absolute path");
                }
                Some(Self { url, key_file })
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
