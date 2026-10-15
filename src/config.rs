use anyhow::{Context, Result, ensure};
use std::{env, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub port: u16,
    pub api_key: Option<String>,
    pub model: String,
    pub model_base_url: String,
    pub context_bytes: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let config = Self {
            data_dir: env::var_os("AYATI_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".ayati")),
            port: env::var("AYATI_PORT")
                .unwrap_or_else(|_| "8765".into())
                .parse()
                .context("AYATI_PORT must be a port number")?,
            api_key: env::var("FIREWORKS_API_KEY")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            model: env::var("AYATI_MODEL")
                .unwrap_or_else(|_| "accounts/fireworks/models/glm-5p3-flash".into()),
            model_base_url: env::var("AYATI_MODEL_BASE_URL")
                .unwrap_or_else(|_| "https://api.fireworks.ai/inference/v1".into()),
            context_bytes: env::var("AYATI_CONTEXT_BYTES")
                .unwrap_or_else(|_| "96000".into())
                .parse()
                .context("AYATI_CONTEXT_BYTES must be a number")?,
        };
        ensure!(config.port != 0, "AYATI_PORT cannot be zero");
        ensure!(
            config.context_bytes >= 16000,
            "AYATI_CONTEXT_BYTES must be at least 16000"
        );
        let url = reqwest::Url::parse(&config.model_base_url).context("Invalid model base URL")?;
        ensure!(
            url.scheme() == "https"
                || (url.scheme() == "http"
                    && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))),
            "Model base URL must use HTTPS (HTTP allowed only for localhost testing)"
        );
        ensure!(
            url.username().is_empty() && url.password().is_none() && url.query().is_none(),
            "Do not put credentials or query parameters in the model base URL"
        );
        Ok(config)
    }
}
