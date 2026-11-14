//! The operator's Telegram file (`AYATI_TELEGRAM_FILE`, controller only): Cuckoo's
//! manager bot, the one bot that creates and manages every agent's bot, and
//! where its token is kept. Read on every use, so the token can be rotated
//! without a restart.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{path::PathBuf, sync::OnceLock};

/// The server name saved on Telegram endpoints.
pub(crate) const SERVER: &str = "telegram";
const API: &str = "https://api.telegram.org";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    /// The manager bot's username, without `@`, e.g. `CuckooAgentsBot`.
    pub manager_username: String,
    /// An owned 0600 file holding the manager bot's token.
    pub manager_token_file: PathBuf,
    /// The Bot API base. Only for local tests: a loopback address.
    #[serde(default)]
    pub api: Option<String>,
}

static FILE: OnceLock<Option<PathBuf>> = OnceLock::new();

pub(crate) fn configure(file: Option<PathBuf>) {
    let _ = FILE.set(file);
}

/// The current settings, or `None` when this server offers no Telegram.
pub(crate) fn current() -> Result<Option<Settings>> {
    match FILE.get() {
        Some(Some(file)) => read(file).map(Some),
        _ => Ok(None),
    }
}

pub(crate) fn read(file: &std::path::Path) -> Result<Settings> {
    let settings: Settings = serde_json::from_str(&crate::identity::private_config::read(file)?)
        .map_err(|_| anyhow::anyhow!("Invalid Telegram configuration"))?;
    settings.validate()?;
    Ok(settings)
}

impl Settings {
    fn validate(&self) -> Result<()> {
        ensure!(
            bot_username(&self.manager_username),
            "Invalid Telegram manager bot username"
        );
        ensure!(
            self.manager_token_file.is_absolute(),
            "Use an absolute manager token path"
        );
        if let Some(api) = &self.api {
            ensure!(
                api.starts_with("http://127.0.0.1:"),
                "A Telegram API override must be a loopback test server"
            );
        }
        Ok(())
    }
    pub fn token(&self) -> Result<String> {
        let token = crate::identity::private_config::read(&self.manager_token_file)
            .context("The Telegram manager token is unavailable")?;
        let token = token.trim();
        ensure!(bot_token(token), "Invalid Telegram manager token");
        Ok(token.to_owned())
    }
    pub fn api(&self) -> &str {
        self.api.as_deref().unwrap_or(API)
    }
}

/// A bot username: 5-32 Latin letters, digits and underscores, ending in `bot`.
pub(crate) fn bot_username(value: &str) -> bool {
    (5..=32).contains(&value.len())
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && value.to_ascii_lowercase().ends_with("bot")
}

/// A bot token: the bot's numeric ID, a colon and its secret.
pub(crate) fn bot_token(value: &str) -> bool {
    let Some((id, secret)) = value.split_once(':') else {
        return false;
    };
    !id.is_empty()
        && id.len() <= 20
        && id.chars().all(|c| c.is_ascii_digit())
        && (20..=100).contains(&secret.len())
        && secret
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_and_tokens_have_telegrams_shapes() {
        assert!(bot_username("CuckooAgentsBot"));
        assert!(bot_username("nova_k7q2_bot"));
        for bad in [
            "bot",
            "nova_agent",
            "nova-bot",
            "a_very_long_username_for_a_bot_bot",
        ] {
            assert!(!bot_username(bad), "{bad}");
        }
        assert!(bot_token("123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw"));
        for bad in [
            "123456789",
            ":AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw",
            "12a:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw",
            "123:short",
            "123:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw with space",
        ] {
            assert!(!bot_token(bad), "{bad}");
        }
    }

    #[test]
    fn the_file_names_a_loopback_api_only_for_tests() {
        let dir = tempfile::tempdir().unwrap();
        let write = |body: &str| {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.path().join("telegram.json");
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            path
        };
        let good = write(
            r#"{"manager_username":"CuckooAgentsBot","manager_token_file":"/private/token"}"#,
        );
        assert_eq!(read(&good).unwrap().api(), API);
        let remote = write(
            r#"{"manager_username":"CuckooAgentsBot","manager_token_file":"/private/token","api":"https://example.com"}"#,
        );
        assert!(read(&remote).is_err());
        let relative =
            write(r#"{"manager_username":"CuckooAgentsBot","manager_token_file":"token"}"#);
        assert!(read(&relative).is_err());
        let unknown =
            write(r#"{"manager_username":"CuckooAgentsBot","manager_token_file":"/t","x":1}"#);
        assert!(read(&unknown).is_err());
    }
}
