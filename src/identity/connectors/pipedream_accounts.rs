//! Pipedream accounts: each Cuckoo workspace is one external user, and only
//! that user's accounts are ever listed, used or removed. Account answers can
//! carry credentials, so only the id, name, app, health and update time are read.
use super::Refusal;
use super::pipedream::Pipedream;
use super::pipedream_api::{Reply, id};
use anyhow::{Context, Result, ensure};
use reqwest::Method;
use serde_json::{Map, Value, json};

/// More accounts than this for one app and workspace can't be told apart.
const MAX_ACCOUNTS: usize = 50;

/// Pipedream's identity for a connected account, saved on the connection.
/// Built the same way every time, so reconnecting the account Pipedream
/// updated in place keeps the same binding and nothing is revoked.
pub(super) struct Binding {
    pub account: String,
    pub app: String,
    pub user: String,
}
impl Binding {
    pub(super) fn parse(value: Option<&Value>, workspace: &str) -> Result<Self> {
        let value = value.context(Refusal::Changed)?;
        let text = |key: &str| -> Result<String> {
            Ok(id(value[key].as_str().context(Refusal::Changed)?)?.to_owned())
        };
        let binding = Self {
            account: text("account")?,
            app: text("app")?,
            user: text("user")?,
        };
        ensure!(binding.user == user(workspace), Refusal::Changed);
        Ok(binding)
    }
}

/// Pipedream keeps accounts per external user; each Cuckoo workspace is one.
pub(super) fn user(workspace: &str) -> String {
    format!("cuckoo-{workspace}")
}

/// An account as Pipedream lists it.
#[derive(Clone)]
pub(super) struct Remote {
    pub id: String,
    pub name: Option<String>,
    healthy: bool,
    updated: Option<String>,
}

impl Pipedream {
    /// The external user's accounts for `app`. An account Pipedream lists for
    /// another external user or app is dropped, whatever the filter did.
    async fn accounts(&self, user: &str, app: &str) -> Result<Vec<Remote>> {
        let list = self
            .api
            .request(
                Method::GET,
                "accounts",
                &[("external_user_id", user), ("app", app), ("limit", "100")],
                None,
            )
            .await?;
        Ok(list["data"]
            .as_array()
            .context("Pipedream omitted the accounts")?
            .iter()
            .filter(|a| a["app"]["name_slug"] == app)
            .filter(|a| a.get("external_id").is_none_or(|e| e == user))
            .filter_map(|a| {
                Some(Remote {
                    id: id(a["id"].as_str()?).ok()?.to_owned(),
                    name: a["name"].as_str().map(|n| n.chars().take(200).collect()),
                    healthy: a["healthy"] == true && a["dead"] != true,
                    updated: a["updated_at"].as_str().map(str::to_owned),
                })
            })
            .collect())
    }

    /// Whether the saved account is still this external user's, for its app,
    /// and healthy at Pipedream.
    pub(super) async fn healthy(&self, binding: &Binding) -> Result<bool> {
        Ok(self
            .accounts(&binding.user, &binding.app)
            .await?
            .iter()
            .any(|a| a.id == binding.account && a.healthy))
    }

    /// The accounts and their `updated_at` before a sign-in, so that finishing
    /// can tell which account it connected.
    pub(super) async fn known(&self, user: &str, app: &str) -> Result<Map<String, Value>> {
        let known: Map<String, Value> = self
            .accounts(user, app)
            .await?
            .into_iter()
            .map(|a| (a.id, json!(a.updated)))
            .collect();
        ensure!(
            known.len() <= MAX_ACCOUNTS,
            "Too many accounts for this app at Pipedream; remove unused ones first"
        );
        Ok(known)
    }

    /// The account a sign-in connected: the one healthy account that wasn't in
    /// `known`, or else the one updated since. Pipedream may update an existing
    /// account in place when the same upstream account signs in again. No
    /// clocks are compared.
    pub(super) async fn signed_in(
        &self,
        user: &str,
        app: &str,
        known: &Map<String, Value>,
    ) -> Result<Option<Remote>> {
        let accounts = self.accounts(user, app).await?;
        let healthy = || accounts.iter().filter(|a| a.healthy);
        let new: Vec<&Remote> = healthy().filter(|a| !known.contains_key(&a.id)).collect();
        let updated: Vec<&Remote> = healthy()
            .filter(|a| {
                known
                    .get(&a.id)
                    .is_some_and(|before| before.as_str() != a.updated.as_deref())
            })
            .collect();
        for found in [new, updated] {
            match found.as_slice() {
                [] => continue,
                [one] => return Ok(Some((*one).clone())),
                _ => {
                    anyhow::bail!("Several accounts were connected at once. Try connecting again.")
                }
            }
        }
        Ok(None)
    }

    /// Deletes the saved account and its credentials at Pipedream, if it is
    /// still this external user's. One that is already gone succeeds.
    pub(super) async fn remove_account(&self, binding: &Binding) -> Result<()> {
        let accounts = self.accounts(&binding.user, &binding.app).await?;
        if !accounts.iter().any(|a| a.id == binding.account) {
            return Ok(());
        }
        let path = format!("accounts/{}", binding.account);
        match self.api.send(Method::DELETE, &path, &[], None).await? {
            Reply::Ok(_) | Reply::Status(404) => Ok(()),
            Reply::Status(code) => {
                anyhow::bail!("Pipedream did not remove the account (HTTP {code})")
            }
        }
    }
}

/// The app's own account identity, when Pipedream names the account by its
/// email address (its documented examples do, for Google apps).
pub(super) fn mailbox(name: Option<&str>) -> Option<String> {
    let name = name?.trim();
    let (local, domain) = name.split_once('@')?;
    (!local.is_empty()
        && domain.contains('.')
        && !domain.contains('@')
        && name.len() <= 254
        && !name.chars().any(|c| c.is_whitespace() || c.is_control()))
    .then(|| name.to_lowercase())
}
