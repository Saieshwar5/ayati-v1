//! `AYATI_CONNECTOR_PROVIDERS_FILE`: which connector providers this controller
//! uses, their key files, prices and the providers per catalog app. Read on every
//! use, like the web search and computer providers files, so keys rotate and
//! routes change in place; checked at controller start, which refuses a broken file.
//!
//! ```json
//! {"providers": [{"provider": "composio", "key_file": "/run/secrets/composio",
//!                 "webhook_secret_file": "/run/secrets/composio-webhook",
//!                 "auth_configs": {"gmail": "ac_123"}, "usd_per_1k_calls": 0.5}],
//!  "routes": {"gmail": ["composio", "pipedream"]}}
//! ```
use super::{composio_api, curated};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProvidersFile {
    #[serde(default)]
    pub providers: Vec<Entry>,
    /// Providers per catalog app, first choice first. Apps not listed use the
    /// catalog's default: the first configured provider, Composio preferred.
    #[serde(default)]
    pub routes: BTreeMap<String, Vec<String>>,
}

/// One provider, named by its `provider` field.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub(crate) enum Entry {
    Composio(Composio),
    Pipedream(Pipedream),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct Composio {
    /// A private file holding only the project API key.
    pub key_file: PathBuf,
    /// A private file holding the project's webhook secret, for provider events.
    #[serde(default)]
    pub webhook_secret_file: Option<PathBuf>,
    /// Toolkit to auth config, for apps that sign in with your own OAuth app.
    #[serde(default)]
    pub auth_configs: BTreeMap<String, String>,
    #[serde(default)]
    pub usd_per_1k_calls: f64,
}

/// Pipedream Connect's project. Parsed and checked here; the provider itself is
/// registered by its own package.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pipedream {
    pub client_id_file: PathBuf,
    pub client_secret_file: PathBuf,
    pub project_id: String,
    /// `development` or `production`.
    pub environment: String,
    #[serde(default)]
    pub usd_per_1k_calls: f64,
}

impl Entry {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Composio(_) => "composio",
            Self::Pipedream(_) => "pipedream",
        }
    }
    fn usd_per_1k_calls(&self) -> f64 {
        match self {
            Self::Composio(c) => c.usd_per_1k_calls,
            Self::Pipedream(p) => p.usd_per_1k_calls,
        }
    }
    /// The private files this entry names.
    fn files(&self) -> Vec<(&'static str, &Path)> {
        match self {
            Self::Composio(c) => std::iter::once(("key_file", c.key_file.as_path()))
                .chain(
                    c.webhook_secret_file
                        .as_deref()
                        .map(|f| ("webhook_secret_file", f)),
                )
                .collect(),
            Self::Pipedream(p) => vec![
                ("client_id_file", p.client_id_file.as_path()),
                ("client_secret_file", p.client_secret_file.as_path()),
            ],
        }
    }
}

impl ProvidersFile {
    /// Reads and checks the file. It must be an owned 0600 file.
    pub fn read(path: &Path) -> Result<Self> {
        let text = crate::identity::private_config::read(path)
            .context("The connector providers file is unavailable")?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let file: Self = serde_json::from_str(text).context("Invalid connector providers file")?;
        let problems = file.problems();
        ensure!(
            problems.is_empty(),
            "Invalid connector providers file: {}",
            problems.join("; ")
        );
        Ok(file)
    }

    /// Every problem in the file's contents, so one edit fixes them all.
    fn problems(&self) -> Vec<String> {
        let mut problems = vec![];
        for (index, entry) in self.providers.iter().enumerate() {
            let name = entry.name();
            if self.providers[..index].iter().any(|e| e.name() == name) {
                problems.push(format!("{name} is listed twice"));
            }
            let price = entry.usd_per_1k_calls();
            if !(price.is_finite() && (0.0..=1000.0).contains(&price)) {
                problems.push(format!(
                    "{name}: usd_per_1k_calls must be 0 to 1,000 dollars"
                ));
            }
            for (field, file) in entry.files() {
                if !file.is_absolute() {
                    problems.push(format!("{name}: {field} must be an absolute path"));
                }
            }
            match entry {
                Entry::Composio(c) => {
                    for (toolkit, config) in &c.auth_configs {
                        if composio_api::id(toolkit).is_err() || composio_api::id(config).is_err() {
                            problems.push(format!("composio: invalid auth config for {toolkit}"));
                        }
                    }
                }
                Entry::Pipedream(p) => {
                    if composio_api::id(&p.project_id).is_err() {
                        problems.push("pipedream: invalid project_id".into());
                    }
                    if !["development", "production"].contains(&p.environment.as_str()) {
                        problems.push("pipedream: environment is development or production".into());
                    }
                }
            }
        }
        for (app, order) in &self.routes {
            if curated::app(app).is_none() {
                problems.push(format!("routes name {app}, which is not a catalog app"));
            }
            if order.is_empty() || order.len() > 4 {
                problems.push(format!("the route for {app} needs 1 to 4 providers"));
            }
            for (index, name) in order.iter().enumerate() {
                if order[..index].contains(name) {
                    problems.push(format!("the route for {app} names {name} twice"));
                } else if !self.providers.iter().any(|e| e.name() == name) {
                    problems.push(format!(
                        "the route for {app} names {name}, which is not in providers"
                    ));
                }
            }
        }
        problems
    }

    /// At controller start: every private file the providers name must be an
    /// owned 0600 file. Their contents are never shown.
    pub fn check_files(&self) -> Result<()> {
        let mut problems = vec![];
        for entry in &self.providers {
            for (field, file) in entry.files() {
                if crate::identity::private_config::read(file).is_err() {
                    problems.push(format!(
                        "{}: {field} must be an owned 0600 file under 64 KiB",
                        entry.name()
                    ));
                }
            }
        }
        ensure!(
            problems.is_empty(),
            "Connector providers file: {}",
            problems.join("; ")
        );
        Ok(())
    }

    pub fn composio(&self) -> Option<&Composio> {
        self.providers.iter().find_map(|e| match e {
            Entry::Composio(c) => Some(c),
            _ => None,
        })
    }

    pub fn pipedream(&self) -> Option<&Pipedream> {
        self.providers.iter().find_map(|e| match e {
            Entry::Pipedream(p) => Some(p),
            _ => None,
        })
    }

    /// US dollars for one call through `provider`; 0 when it has no price.
    pub fn price_per_call(&self, provider: &str) -> f64 {
        self.providers
            .iter()
            .find(|e| e.name() == provider)
            .map_or(0.0, |e| e.usd_per_1k_calls() / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const GOOD: &str = r#"{
      "providers": [
        {"provider": "composio", "key_file": "/run/secrets/composio",
         "webhook_secret_file": "/run/secrets/composio-webhook",
         "auth_configs": {"gmail": "ac_123"}, "usd_per_1k_calls": 0.5},
        {"provider": "pipedream", "client_id_file": "/run/secrets/pd-id",
         "client_secret_file": "/run/secrets/pd-secret", "project_id": "proj_abc",
         "environment": "production", "usd_per_1k_calls": 0}
      ],
      "routes": {"gmail": ["composio", "pipedream"]}
    }"#;

    fn error(text: &str) -> String {
        ProvidersFile::parse(text).err().unwrap().to_string()
    }

    #[test]
    fn the_contract_example_is_valid_and_answers_settings_prices_and_pipedream() {
        let file = ProvidersFile::parse(GOOD).unwrap();
        let composio = file.composio().unwrap();
        assert_eq!(composio.auth_configs["gmail"], "ac_123");
        assert_eq!(
            composio.webhook_secret_file.as_deref(),
            Some(Path::new("/run/secrets/composio-webhook"))
        );
        assert_eq!(file.price_per_call("composio"), 0.0005);
        assert_eq!(file.price_per_call("mcp"), 0.0);
        let pipedream = file.pipedream().unwrap();
        assert_eq!(
            (
                pipedream.project_id.as_str(),
                pipedream.environment.as_str()
            ),
            ("proj_abc", "production")
        );
        assert_eq!(file.routes["gmail"], ["composio", "pipedream"]);
        // Providers and routes are optional.
        assert!(ProvidersFile::parse("{}").unwrap().composio().is_none());
    }

    #[test]
    fn broken_files_are_refused_with_every_problem_named() {
        // Unknown providers and fields are refused.
        assert!(
            error(r#"{"providers":[{"provider":"zapier","key_file":"/k"}]}"#).contains("Invalid")
        );
        assert!(
            ProvidersFile::parse(
                r#"{"providers":[{"provider":"composio","key_file":"/k","base":"x"}]}"#
            )
            .is_err()
        );
        assert!(ProvidersFile::parse(r#"{"provider":[]}"#).is_err());
        let text = r#"{"providers":[
            {"provider":"composio","key_file":"relative","usd_per_1k_calls":-1},
            {"provider":"composio","key_file":"/k"},
            {"provider":"pipedream","client_id_file":"/a","client_secret_file":"/b","project_id":"p/1","environment":"staging"}],
          "routes":{"gmail":["composio","mcp"],"slackish":["composio"],"google_drive":[]}}"#;
        let problems = error(text);
        for expected in [
            "composio is listed twice",
            "usd_per_1k_calls",
            "key_file must be an absolute path",
            "invalid project_id",
            "development or production",
            "names mcp, which is not in providers",
            "slackish, which is not a catalog app",
            "google_drive needs 1 to 4 providers",
        ] {
            assert!(problems.contains(expected), "{expected}: {problems}");
        }
        assert!(error(r#"{"providers":[{"provider":"composio","key_file":"/k"}],"routes":{"gmail":["pipedream"]}}"#)
            .contains("names pipedream, which is not in providers"));
    }

    #[test]
    fn the_file_and_its_key_files_must_be_private() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("composio");
        std::fs::write(&key, "ak_project").unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
        let path = dir.path().join("connectors.json");
        let text = format!(r#"{{"providers":[{{"provider":"composio","key_file":{key:?}}}]}}"#);
        std::fs::write(&path, &text).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            ProvidersFile::read(&path).is_err(),
            "a shared file is refused"
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let file = ProvidersFile::read(&path).unwrap();
        file.check_files().unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o640)).unwrap();
        let problem = file.check_files().err().unwrap().to_string();
        assert!(problem.contains("composio: key_file") && !problem.contains("ak_project"));
    }
}
