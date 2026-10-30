use anyhow::{Context, Result, ensure};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use tokio::process::Command;

pub const GOOSE_VERSION: &str = "1.54.0";
pub const PREPARED_GOOSE_VERSION: &str = "1.54.0+ayati.1";

#[derive(Clone)]
pub struct GooseConfig {
    pub(crate) control_tools: Option<crate::identity::control::broker::Endpoint>,
    pub(crate) trace_assignment: Option<String>,
    pub binary: PathBuf,
    pub agent_id: String,
    pub role: String,
    pub root: PathBuf,
    pub workspace: PathBuf,
    pub provider: String,
    pub model: String,
    pub run_timeout: std::time::Duration,
}

impl GooseConfig {
    pub fn new(data: &Path, model: &str, goose: &crate::config::GooseSettings) -> Self {
        Self {
            control_tools: None,
            trace_assignment: None,
            agent_id: "local-agent".into(),
            role: String::new(),
            binary: goose.binary.clone(),
            root: data.join("goose"),
            workspace: data.join("workspace"),
            provider: goose.provider.clone(),
            model: model.into(),
            run_timeout: std::time::Duration::from_secs(180),
        }
    }

    pub fn configured(&self) -> bool {
        let key = if self.provider == "openai" {
            "OPENAI_API_KEY"
        } else {
            "FIREWORKS_API_KEY"
        };
        self.binary.is_file() && std::env::var(key).is_ok_and(|s| !s.trim().is_empty())
    }

    pub async fn command(&self) -> Result<Command> {
        self.command_for(false).await
    }
    pub(crate) async fn prepared_command(&self) -> Result<Command> {
        self.command_for(true).await
    }
    async fn command_for(&self, prepared: bool) -> Result<Command> {
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
        let expected = if prepared {
            PREPARED_GOOSE_VERSION
        } else {
            GOOSE_VERSION
        };
        ensure!(
            version.status.success() && String::from_utf8_lossy(&version.stdout).trim() == expected,
            "Expected pinned Goose {expected}; install the tested artifact"
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
        let isolated_root = self.root.canonicalize()?;
        // Pinned Goose discovers ancestor AGENTS.md/.goosehints by default.
        // Agents receive context only through the controller.
        command
            .env("CONTEXT_FILE_NAMES", "[]")
            .env("HOME", &isolated_root)
            .env("GOOSE_PATH_ROOT", &isolated_root)
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
        let mut extensions = Vec::new();
        if self.control_tools.is_some() {
            extensions.push(serde_json::json!({"type":"mcp","server":{"name":"ayati_control","command":std::env::current_exe()?,"args":["control-mcp"],"env":[]}}));
        }
        Ok(
            serde_json::json!({"enabledExtensions":extensions,"sessionTitle":"Ayati local agent"})
                .as_object()
                .unwrap()
                .clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn every_command_disables_host_hint_discovery_in_an_isolated_home() {
        for prepared in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let goose = crate::config::GooseSettings {
                binary: directory.path().join("fixture-goose"),
                provider: "fireworks-ai".into(),
            };
            let config = GooseConfig::new(directory.path(), "fixture", &goose);
            let version = if prepared {
                PREPARED_GOOSE_VERSION
            } else {
                GOOSE_VERSION
            };
            std::fs::write(
                &config.binary,
                format!("#!/bin/sh\nprintf '%s\\n' '{version}'\n"),
            )
            .unwrap();
            std::fs::set_permissions(&config.binary, std::fs::Permissions::from_mode(0o700))
                .unwrap();
            let command = config.command_for(prepared).await.unwrap();
            let env = |name: &str| {
                command
                    .as_std()
                    .get_envs()
                    .find(|(key, _)| *key == name)
                    .and_then(|(_, value)| value)
                    .map(PathBuf::from)
            };
            let root = config.root.canonicalize().unwrap();
            assert_eq!(env("GOOSE_PATH_ROOT"), Some(root.clone()));
            assert_eq!(env("HOME"), Some(root));
            assert_eq!(env("CONTEXT_FILE_NAMES"), Some(PathBuf::from("[]")));
        }
    }
}
