//! Cuckoo Control: the operator console's HTTP API (`routes`) and `ayati admin`
//! (`cli`). Both call these actions, so validation and the change log are the
//! same whichever one an operator uses. See runtime/cuckoo-control.md.
pub mod cli;
mod cli_args;
pub(crate) mod routes;
#[cfg(test)]
mod routes_tests;

use super::postgres::{
    PgStore,
    operator::{Operator, models::Route, providers},
};
use super::secrets::Sealer;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::Path;

/// What actions need besides the database: the deployment's files.
pub(crate) struct Deps<'a> {
    pub store: &'a PgStore,
    pub providers_file: Option<&'a Path>,
    pub secrets_key_file: Option<&'a Path>,
}

impl Deps<'_> {
    fn sealer(&self) -> Result<Sealer> {
        self.secrets_key_file.map(Sealer::new).context(
            "Set AYATI_SECRETS_KEY_FILE in the deployment to store provider keys in Cuckoo Control",
        )
    }

    fn file_providers(&self) -> Vec<providers::FileProvider> {
        super::worker_control::file_providers(self.providers_file)
    }

    pub(crate) async fn providers(&self) -> Result<Value> {
        let mut data = self
            .store
            .operator_providers(&self.file_providers())
            .await?;
        data["can_store_keys"] = json!(self.secrets_key_file.is_some());
        Ok(data)
    }

    pub(crate) async fn add_provider(
        &self,
        by: &Operator,
        name: &str,
        base_url: &str,
        key: &str,
        why: &str,
    ) -> Result<()> {
        let why = super::postgres::operator::reason(why)?;
        let name = providers::name(name)?;
        let base_url = providers::base_url(base_url)?;
        let hint = providers::hint(key)?;
        ensure!(
            !self.file_providers().iter().any(|p| p.name == name),
            "{name} is already set up in the deployment's providers file"
        );
        let sealed = self.sealer()?.seal(&name, key.trim())?;
        self.store
            .operator_add_provider(by, &name, &base_url, sealed, &hint, &why)
            .await
    }

    pub(crate) async fn rotate_key(
        &self,
        by: &Operator,
        name: &str,
        key: &str,
        why: &str,
    ) -> Result<()> {
        let why = super::postgres::operator::reason(why)?;
        let hint = providers::hint(key)?;
        let sealed = self.sealer()?.seal(name, key.trim())?;
        self.store
            .operator_rotate_key(by, name, sealed, &hint, &why)
            .await
    }

    /// Routes may only name providers Cuckoo can call.
    pub(crate) async fn set_route(&self, by: &Operator, route: &Route, why: &str) -> Result<()> {
        let why = super::postgres::operator::reason(why)?;
        let known = self.providers().await?;
        for target in std::iter::once(&route.main).chain(route.second.as_ref()) {
            let found = known["providers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|p| p["name"] == target.provider.as_str() && p["source"] != "missing");
            ensure!(
                found,
                "{} is not a configured model provider",
                target.provider
            );
        }
        self.store.operator_set_route(by, route, &why).await
    }

    pub(crate) async fn switch_provider(
        &self,
        by: &Operator,
        name: &str,
        enabled: bool,
        why: &str,
    ) -> Result<()> {
        let why = super::postgres::operator::reason(why)?;
        let name = providers::name(name)?;
        self.store
            .operator_switch_provider(by, &name, enabled, &why)
            .await
    }
}

/// Settings the deployment sets, shown read-only beside the console's own.
pub(crate) fn deployment_settings(config: &crate::config::Controller) -> Value {
    let file = |path: &Option<std::path::PathBuf>| path.as_ref().map(|p| p.display().to_string());
    json!([
        {"key": "public_origin", "env": "AYATI_PUBLIC_ORIGIN", "value": config.origin, "about": "The address people open"},
        {"key": "platform_admins", "env": "AYATI_PLATFORM_ADMINS", "value": format!("{} people", config.platform_admins.len()),
            "about": "Who can open Cuckoo Control. Listed in the deployment so nobody can add themselves here"},
        {"key": "control_environment", "env": "AYATI_CONTROL_ENVIRONMENT", "value": config.control_environment, "about": "The name in Cuckoo Control's environment band"},
        {"key": "default_model", "env": "AYATI_MODEL", "value": config.default_model, "about": "The model new agents start with"},
        {"key": "model_providers_file", "env": "AYATI_MODEL_PROVIDERS_FILE", "value": file(&config.model_providers_file), "about": "Providers the deployment configures"},
        {"key": "secrets_key_file", "env": "AYATI_SECRETS_KEY_FILE", "value": config.secrets_key_file.as_ref().map(|_| "set"), "about": "Seals provider keys added in the console"},
        {"key": "executor_capacity", "env": "AYATI_EXECUTOR_CAPACITY", "value": config.pool.capacity, "about": "Agent work running at once"},
        {"key": "chat_reserve", "env": "AYATI_EXECUTOR_CHAT_RESERVE", "value": config.pool.chat_reserve, "about": "Slots kept free for Chat replies"},
        {"key": "workspace_limit", "env": "AYATI_EXECUTOR_WORKSPACE_LIMIT", "value": config.pool.workspace_limit, "about": "Most slots one workspace may hold"},
        {"key": "run_seconds", "env": "AYATI_ASSIGNMENT_RUN_SECONDS", "value": config.pool.run_seconds, "about": "Longest one agent run may take"},
        {"key": "retain_sync_days", "env": "AYATI_RETAIN_SYNC_DAYS", "value": config.retain_sync_days, "about": "Days live-update notices are kept"},
    ])
}
