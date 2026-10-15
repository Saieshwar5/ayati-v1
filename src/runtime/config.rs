use anyhow::{Context, Result, ensure};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use tokio::process::Command;

pub const GOOSE_VERSION: &str = "1.54.0";

#[derive(Clone)]
pub struct GooseConfig {
    pub binary: PathBuf,
    pub root: PathBuf,
    pub workspace: PathBuf,
    pub provider: String,
    pub model: String,
    pub proof_tool: bool,
}

impl GooseConfig {
    pub fn from_env(data: &Path, model: &str) -> Self {
        Self {
            binary: std::env::var_os("AYATI_GOOSE_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".cache/goose/v1.54.0/goose")),
            root: data.join("goose"),
            workspace: data.join("workspace"),
            provider: std::env::var("AYATI_GOOSE_PROVIDER")
                .unwrap_or_else(|_| "fireworks-ai".into()),
            model: model.into(),
            proof_tool: std::env::var("AYATI_PROOF_TOOL").as_deref() == Ok("1"),
        }
    }

    pub async fn command(&self) -> Result<Command> {
        for path in [&self.root, &self.workspace, &self.root.join("config")] {
            std::fs::create_dir_all(path)?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        }
        let binary = self
            .binary
            .canonicalize()
            .context("Goose is not installed; run tools/install-goose.sh")?;
        let version = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            Command::new(&binary)
                .arg("--version")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .context("Goose version check timed out")??;
        ensure!(
            version.status.success()
                && String::from_utf8_lossy(&version.stdout).trim() == GOOSE_VERSION,
            "Expected pinned Goose 1.54.0; install the tested artifact"
        );
        let mut command = Command::new(binary);
        command.arg("acp").current_dir(&self.workspace).env_clear();
        // Never inherit unrelated credentials or the owner's Goose configuration.
        for key in ["PATH", "LANG", "SSL_CERT_FILE", "SSL_CERT_DIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let key = match self.provider.as_str() {
            "fireworks-ai" => "FIREWORKS_API_KEY",
            "openai" => "OPENAI_API_KEY",
            _ => anyhow::bail!("This local proof supports fireworks-ai and openai providers only"),
        };
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
        // Useful for explicit OpenAI-compatible local fixtures; never forwarded to Fireworks.
        if self.provider == "openai" {
            for name in ["OPENAI_HOST", "OPENAI_BASE_PATH"] {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
        }
        command
            .env("GOOSE_PATH_ROOT", &self.root)
            .env("GOOSE_PROVIDER", &self.provider)
            .env("GOOSE_MODEL", &self.model)
            .env("GOOSE_MODE", "approve")
            .env("GOOSE_DISABLE_KEYRING", "1")
            .env("GOOSE_DISABLE_SESSION_NAMING", "true")
            .env("DO_NOT_TRACK", "1");
        Ok(command)
    }

    // Goose 1.54 treats this explicit selection as authoritative. Omitting it
    // can enable default developer/configured tools; isolate it for upgrade tests.
    pub fn session_meta(&self) -> Result<serde_json::Map<String, serde_json::Value>> {
        let extensions = if self.proof_tool {
            serde_json::json!([{"type":"mcp","server": {
                "name":"ayati_proof", "command":std::env::current_exe()?,
                "args":["mcp"], "env":[]
            }}])
        } else {
            serde_json::json!([])
        };
        Ok(
            serde_json::json!({"enabledExtensions":extensions,"sessionTitle":"Ayati local agent"})
                .as_object()
                .unwrap()
                .clone(),
        )
    }
}
