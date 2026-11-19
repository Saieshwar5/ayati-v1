//! A Composio account's sign-in configuration, its state, and the tools read
//! for it. Only the fields Cuckoo needs are kept from Composio's answers.
use super::Refusal;
use super::composio::{Binding, Composio, State};
use super::composio_api::{self, Reply};
use anyhow::{Context, Result, ensure};
use reqwest::Method;
use serde_json::{Value, json};

impl Composio {
    /// The auth config to sign in with for `toolkit`.
    pub(super) async fn auth_config(&self, toolkit: &str) -> Result<String> {
        if let Some(config) = self.auth_configs.get(toolkit) {
            return Ok(config.clone());
        }
        let list = self
            .api
            .request(
                Method::GET,
                &format!("auth_configs?toolkit_slug={toolkit}&limit=50"),
                None,
            )
            .await?;
        let enabled: Vec<&str> = list["items"]
            .as_array()
            .context("Composio omitted auth configs")?
            .iter()
            .filter(|c| c["status"] == "ENABLED" && slug_is(&c["toolkit"]["slug"], toolkit))
            .filter_map(|c| c["id"].as_str())
            .collect();
        match enabled.as_slice() {
            [one] => Ok(composio_api::id(one)?.to_owned()),
            [] => {
                let created = self
                    .api
                    .request(
                        Method::POST,
                        "auth_configs",
                        Some(&json!({"toolkit":{"slug":toolkit},"auth_config":{"type":"use_composio_managed_auth"}})),
                    )
                    .await?;
                Ok(composio_api::id(
                    created["auth_config"]["id"]
                        .as_str()
                        .context("Composio omitted the auth config")?,
                )?
                .to_owned())
            }
            _ => anyhow::bail!(
                "Several Composio auth configs exist for {toolkit}; choose one in AYATI_COMPOSIO_AUTH_CONFIGS"
            ),
        }
    }

    /// The account's state at Composio. Only the fields Cuckoo needs are read;
    /// the rest of Composio's answer can carry tokens and is dropped here.
    pub(super) async fn state(&self, binding: &Binding, toolkit: &str) -> Result<State> {
        let account = match self
            .api
            .send(
                Method::GET,
                &format!("connected_accounts/{}", binding.account),
                None,
            )
            .await?
        {
            Reply::Ok(account) => account,
            Reply::Status(404) => return Ok(State::Broken),
            Reply::Status(code) => {
                anyhow::bail!("Composio refused the account check (HTTP {code})")
            }
        };
        ensure!(
            account["id"] == binding.account.as_str()
                && slug_is(&account["toolkit"]["slug"], toolkit)
                && account["auth_config"]["id"] == binding.auth_config.as_str(),
            Refusal::Changed
        );
        if account["is_disabled"] == true || account["auth_config"]["is_disabled"] == true {
            return Ok(State::Broken);
        }
        Ok(match account["status"].as_str() {
            Some("ACTIVE") => State::Active,
            Some("INITIALIZING" | "INITIATED") => State::Pending,
            _ => State::Broken,
        })
    }

    pub(super) async fn tool(&self, slug: &str, version: Option<&str>) -> Result<Value> {
        self.find_tool(slug, version)
            .await?
            .context("Composio has no such tool")
    }

    /// The tool at `version` (its newest without one), or `None` when Composio
    /// no longer has it.
    pub(super) async fn find_tool(
        &self,
        slug: &str,
        version: Option<&str>,
    ) -> Result<Option<Value>> {
        let path = match version {
            Some(v) => format!(
                "tools/{}?version={}",
                composio_api::id(slug)?,
                composio_api::id(v)?
            ),
            None => format!("tools/{}", composio_api::id(slug)?),
        };
        match self.api.send(Method::GET, &path, None).await? {
            Reply::Ok(tool) => {
                ensure!(tool["slug"] == slug, "Composio returned a different tool");
                Ok(Some(tool))
            }
            Reply::Status(404) => Ok(None),
            Reply::Status(code) => anyhow::bail!("Composio refused the request (HTTP {code})"),
        }
    }
}

fn slug_is(value: &Value, toolkit: &str) -> bool {
    value
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case(toolkit))
}
