use anyhow::{Context, Result, ensure};
use std::{env, path::PathBuf};

/// Process settings shared by every command. Role-specific settings are read
/// by the controller, executor and migration commands themselves.
#[derive(Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let config = Self {
            data_dir: env::var_os("AYATI_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".ayati-local")),
            port: env::var("AYATI_PORT")
                .unwrap_or_else(|_| "8765".into())
                .parse()
                .context("AYATI_PORT must be a port number")?,
        };
        ensure!(config.port != 0, "AYATI_PORT cannot be zero");
        Ok(config)
    }
}
