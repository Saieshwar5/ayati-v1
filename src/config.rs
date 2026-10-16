use anyhow::{Context, Result, ensure};
use std::{env, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub port: u16,
    pub prototype: bool,
    pub api_key: Option<String>,
    pub model: String,
    pub model_base_url: String,
    pub context_bytes: usize,
    pub vm_idle_seconds: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let runtime = env::var("AYATI_RUNTIME").unwrap_or_else(|_| "goose".into());
        ensure!(
            matches!(runtime.as_str(), "goose" | "prototype"),
            "AYATI_RUNTIME must be goose or prototype"
        );
        let execution = env::var("AYATI_EXECUTION").unwrap_or_else(|_| "host".into());
        ensure!(
            matches!(execution.as_str(), "host" | "vm"),
            "AYATI_EXECUTION must be host or vm"
        );
        ensure!(
            runtime != "prototype" || execution == "host",
            "VM execution requires AYATI_RUNTIME=goose; the prototype only supports host execution"
        );
        let browser = env::var("AYATI_BROWSER").unwrap_or_else(|_| "0".into());
        ensure!(
            ["0", "1"].contains(&browser.as_str()),
            "AYATI_BROWSER must be 0 or 1"
        );
        ensure!(
            browser == "0" || (runtime == "goose" && execution == "vm"),
            "Browser execution requires AYATI_RUNTIME=goose and AYATI_EXECUTION=vm"
        );
        let memory = env::var("AYATI_MEMORY").unwrap_or_else(|_| "0".into());
        ensure!(
            ["0", "1"].contains(&memory.as_str()),
            "AYATI_MEMORY must be 0 or 1"
        );
        ensure!(
            memory == "0" || runtime == "goose",
            "Memory requires AYATI_RUNTIME=goose"
        );
        let config = Self {
            prototype: runtime == "prototype",
            data_dir: env::var_os("AYATI_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".ayati-local")),
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
            vm_idle_seconds: env::var("AYATI_VM_IDLE_SECONDS")
                .unwrap_or_else(|_| "300".into())
                .parse()
                .context("AYATI_VM_IDLE_SECONDS must be 0–86400")?,
        };
        ensure!(config.port != 0, "AYATI_PORT cannot be zero");
        ensure!(
            config.vm_idle_seconds <= 86400,
            "AYATI_VM_IDLE_SECONDS must be 0–86400"
        );
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
