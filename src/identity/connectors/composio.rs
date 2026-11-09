//! Composio as a connector provider. Composio hosts the sign-in pages and keeps
//! the tokens; Cuckoo offers only the actions in its own catalog, reduced to the
//! reviewed fields, and pins the Composio tool version each one was reviewed at.
//! Every call first checks that the account is still active and the pinned tool
//! still means the same thing; the agent never sees Composio.
use super::{
    Account, App, Call, Capabilities, Connect, ConnectorProvider, Delivery, Outcome, Refusal,
    Saved, Subscription,
    composio_api::{self, ComposioApi, Reply},
    composio_triggers,
    curated::{self, CatalogApp},
    fingerprint,
};
use crate::identity::control::model::Operation;
use anyhow::{Context, Result, ensure};
use reqwest::Method;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

const NAME: &str = "composio";
/// Results larger than this are summarized instead of returned to the agent.
const MAX_RESULT: usize = 64 * 1024;
/// Seconds a sign-in page stays usable from Cuckoo's side.
const SIGN_IN_SECONDS: i64 = 15 * 60;

#[derive(Clone)]
pub(crate) struct Settings {
    pub base: String,
    pub key_file: PathBuf,
    /// Toolkit to the auth config to sign in with, for apps that use your own
    /// OAuth app at Composio. Others use the toolkit's single enabled auth
    /// config, or a new Composio-managed one.
    pub auth_configs: BTreeMap<String, String>,
}
impl Settings {
    /// Composio's entry in the connector providers file.
    pub(crate) fn from_entry(entry: &super::providers_file::Composio, base: &str) -> Self {
        Self {
            base: base.into(),
            key_file: entry.key_file.clone(),
            auth_configs: entry.auth_configs.clone(),
        }
    }
}

pub(super) struct Composio {
    api: ComposioApi,
    auth_configs: BTreeMap<String, String>,
}

/// Composio's identity for a connected account, saved on the connection.
struct Binding {
    account: String,
    auth_config: String,
    user: String,
}
impl Binding {
    fn parse(value: Option<&Value>, workspace: &str) -> Result<Self> {
        let value = value.context(Refusal::Changed)?;
        let text = |key: &str| -> Result<String> {
            Ok(composio_api::id(value[key].as_str().context(Refusal::Changed)?)?.to_owned())
        };
        let binding = Self {
            account: text("account")?,
            auth_config: text("auth_config")?,
            user: text("user")?,
        };
        ensure!(binding.user == user(workspace), Refusal::Changed);
        Ok(binding)
    }
}

/// Composio keeps accounts per "user"; each Cuckoo workspace is one.
fn user(workspace: &str) -> String {
    format!("cuckoo-{workspace}")
}

enum State {
    Active,
    Pending,
    Broken,
}

impl Composio {
    pub(super) fn new(settings: Settings) -> Result<Self> {
        for (toolkit, config) in &settings.auth_configs {
            composio_api::id(toolkit)?;
            composio_api::id(config)?;
        }
        Ok(Self {
            api: ComposioApi::new(&settings.base, settings.key_file)?,
            auth_configs: settings.auth_configs,
        })
    }

    fn app(id: &str) -> Result<(&'static CatalogApp, &'static str)> {
        let app = curated::app(id).context("This connector is not offered on this server")?;
        let toolkit = app
            .providers
            .get(NAME)
            .context("Composio does not serve this app")?;
        Ok((app, toolkit.as_str()))
    }

    /// The auth config to sign in with for `toolkit`.
    async fn auth_config(&self, toolkit: &str) -> Result<String> {
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
    async fn state(&self, binding: &Binding, toolkit: &str) -> Result<State> {
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

    async fn tool(&self, slug: &str, version: Option<&str>) -> Result<Value> {
        self.find_tool(slug, version)
            .await?
            .context("Composio has no such tool")
    }

    /// The tool at `version` (its newest without one), or `None` when Composio
    /// no longer has it.
    async fn find_tool(&self, slug: &str, version: Option<&str>) -> Result<Option<Value>> {
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

    /// The catalog's action and Composio binding `operation` was reviewed
    /// against, while this release still maps it to the same tool.
    fn reviewed(
        app: &str,
        operation: &Operation,
    ) -> Option<(&'static curated::Action, &'static curated::Binding)> {
        let (app, _) = Self::app(app).ok()?;
        let action = app.actions.iter().find(|a| a.id == operation.id)?;
        let binding = action.providers.get(NAME)?;
        (binding.tool == operation.tool).then_some((action, binding))
    }

    /// Cuckoo's actions for `app`, each reviewed against Composio's current tool.
    /// Actions Composio can't offer with plain, reviewed fields are left out.
    async fn operations(&self, app: &CatalogApp) -> Result<Vec<Operation>> {
        let mut operations = vec![];
        for action in &app.actions {
            let Some(binding) = action.providers.get(NAME) else {
                continue;
            };
            let tool = self.tool(&binding.tool, None).await?;
            if tool["is_deprecated"] == true {
                continue;
            }
            let (Some(schema), Some(version)) = (
                curated::project(action, binding, &tool["input_parameters"]),
                tool["version"].as_str(),
            ) else {
                continue;
            };
            let operation = Operation {
                id: action.id.clone(),
                title: action.title.clone(),
                tool: binding.tool.clone(),
                effect: action.effect.clone(),
                schema,
                version: Some(composio_api::id(version)?.to_owned()),
            };
            if operation.validate().is_ok() {
                operations.push(operation);
            }
        }
        Ok(operations)
    }
}

impl Composio {
    /// The account's own identity from the app's identity read, such as the
    /// mailbox address. Only that one field is kept from the answer. A read that
    /// fails proves nothing, so the account is saved without an identity.
    async fn identity(&self, binding: &Binding, app: &CatalogApp) -> Option<String> {
        let read = app.identity.get(NAME)?;
        let path = format!("tools/execute/{}", composio_api::id(&read.tool).ok()?);
        let body = json!({"connected_account_id":binding.account,"user_id":binding.user,"arguments":read.arguments});
        match self.api.send(Method::POST, &path, Some(&body)).await {
            Ok(Reply::Ok(reply)) if reply["successful"] == true && reply["error"].is_null() => {
                curated::identity_value(&reply["data"], &read.field)
            }
            _ => None,
        }
    }
}

fn slug_is(value: &Value, toolkit: &str) -> bool {
    value
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case(toolkit))
}

#[async_trait::async_trait]
impl ConnectorProvider for Composio {
    fn name(&self) -> &'static str {
        NAME
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            sign_in: true,
            revoke: true,
            list_fields: true,
            triggers: true,
            ..Capabilities::default()
        }
    }
    fn apps(&self, _workspace: &str) -> Result<Vec<App>> {
        Ok(curated::catalog()
            .apps
            .iter()
            .filter(|app| app.providers.contains_key(NAME))
            .map(|app| App {
                id: app.id.clone(),
                name: app.name.clone(),
                account: None,
                operations: app
                    .actions
                    .iter()
                    .filter_map(|a| {
                        Some(Operation {
                            id: a.id.clone(),
                            title: a.title.clone(),
                            tool: a.providers.get(NAME)?.tool.clone(),
                            effect: a.effect.clone(),
                            schema: json!({}),
                            version: None,
                        })
                    })
                    .collect(),
                provider: NAME,
            })
            .collect())
    }

    async fn connect(&self, workspace: &str, app: &str, callback: &str) -> Result<Connect> {
        let (_, toolkit) = Self::app(app)?;
        let auth_config = self.auth_config(toolkit).await?;
        let link = self
            .api
            .request(
                Method::POST,
                "connected_accounts/link",
                Some(&json!({"auth_config_id":auth_config,"user_id":user(workspace),"callback_url":callback})),
            )
            .await?;
        let account = composio_api::id(
            link["connected_account_id"]
                .as_str()
                .context("Composio omitted the account")?,
        )?;
        let url = reqwest::Url::parse(
            link["redirect_url"]
                .as_str()
                .context("Composio omitted the sign-in page")?,
        )
        .context("Composio returned an invalid sign-in page")?;
        ensure!(
            url.scheme() == "https"
                || (self.api.is_fixture() && url.host_str() == Some("127.0.0.1")),
            "Composio returned an insecure sign-in page"
        );
        Ok(Connect::SignIn {
            binding: json!({"account":account,"auth_config":auth_config,"user":user(workspace)}),
            url: url.into(),
            expires: crate::identity::user::now() + SIGN_IN_SECONDS,
        })
    }

    async fn finish(&self, workspace: &str, app: &str, value: &Value) -> Result<Account> {
        let (catalog, toolkit) = Self::app(app)?;
        let binding = Binding::parse(Some(value), workspace)?;
        // Composio may still be completing the sign-in when the person returns.
        let mut attempt = 0;
        loop {
            match self.state(&binding, toolkit).await? {
                State::Active => break,
                State::Pending if attempt < 3 => attempt += 1,
                _ => anyhow::bail!("The sign-in did not finish. Try connecting again."),
            }
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
        let operations = self.operations(catalog).await?;
        ensure!(
            !operations.is_empty(),
            "Composio offers none of Cuckoo's reviewed actions for {}",
            catalog.name
        );
        let identity = self.identity(&binding, catalog).await;
        let definition =
            json!({"id":catalog.id,"name":catalog.name,"account":identity,"operations":operations});
        Ok(Account {
            fingerprint: fingerprint(NAME, &definition, Some(value)),
            definition,
            binding: Some(value.clone()),
            identity,
        })
    }

    /// The pinned tool and Composio's newest version of it, each reduced to the
    /// reviewed fields as at connecting (`schema` is null when the action can't
    /// be offered any more). Reads tools only; the account is not used.
    async fn describe(&self, saved: &Saved<'_>, operation: &Operation) -> Result<Value> {
        let Some((action, reviewed)) = Self::reviewed(saved.server, operation) else {
            return Ok(json!({"version":operation.version,"schema":null,"latest":null}));
        };
        let reduce = |tool: &Value| curated::project(action, reviewed, &tool["input_parameters"]);
        let pinned = match operation.version.as_deref() {
            Some(version) => self.find_tool(&operation.tool, Some(version)).await?,
            None => None,
        };
        // A tool Composio no longer lists is gone, even while it serves old versions.
        let latest = self.find_tool(&operation.tool, None).await?;
        Ok(json!({
            "version": operation.version,
            "schema": latest.as_ref().and(pinned.as_ref()).and_then(reduce),
            "latest": latest.as_ref().map(|tool| json!({
                "version": tool["version"],
                "schema": if tool["is_deprecated"] == true { None } else { reduce(tool) },
            })),
        }))
    }

    async fn disconnect(&self, workspace: &str, _app: &str, value: Option<&Value>) -> Result<()> {
        let binding = Binding::parse(value, workspace)?;
        let path = format!("connected_accounts/{}", binding.account);
        for (method, path) in [
            (Method::POST, format!("{path}/revoke")),
            (Method::DELETE, path.clone()),
        ] {
            match self.api.send(method, &path, None).await? {
                Reply::Ok(_) | Reply::Status(404) => {}
                Reply::Status(code) => {
                    anyhow::bail!("Composio did not remove the account (HTTP {code})")
                }
            }
        }
        Ok(())
    }

    async fn prepare(&self, saved: &Saved<'_>, operation: &Operation) -> Result<Box<dyn Call>> {
        let binding = Binding::parse(saved.binding, saved.workspace)?;
        let (_, toolkit) = Self::app(saved.server).map_err(|_| Refusal::Changed)?;
        let (action, reviewed) =
            Self::reviewed(saved.server, operation).context(Refusal::Changed)?;
        match self.state(&binding, toolkit).await? {
            State::Active => {}
            _ => return Err(Refusal::NeedsReconnect.into()),
        }
        // A pinned version Composio no longer serves is a changed action, not
        // a provider failure: it must not count toward the circuit breaker.
        let tool = self
            .find_tool(&operation.tool, operation.version.as_deref())
            .await?
            .context(Refusal::Changed)?;
        ensure!(
            curated::project(action, reviewed, &tool["input_parameters"]).as_ref()
                == Some(&operation.schema),
            Refusal::Changed
        );
        Ok(Box::new(ComposioCall {
            api: self.api.clone(),
            body: json!({"connected_account_id":binding.account,"user_id":binding.user,"version":operation.version}),
            tool: operation.tool.clone(),
            fields: reviewed.fields.clone(),
        }))
    }

    /// Only a catalog event of this app, on an active account, is subscribed.
    async fn subscribe(
        &self,
        saved: &Saved<'_>,
        trigger: &str,
        config: &Value,
        _webhook: &str,
    ) -> Result<Subscription> {
        let binding = Binding::parse(saved.binding, saved.workspace)?;
        let (app, toolkit) = Self::app(saved.server).map_err(|_| Refusal::Changed)?;
        ensure!(
            app.triggers
                .iter()
                .any(|t| t.providers.get(NAME).is_some_and(|b| b.tool == trigger)),
            Refusal::Changed
        );
        match self.state(&binding, toolkit).await? {
            State::Active => {}
            _ => return Err(Refusal::NeedsReconnect.into()),
        }
        composio_triggers::subscribe(&self.api, &binding.account, &binding.user, trigger, config)
            .await
    }
    async fn unsubscribe(&self, saved: &Saved<'_>, subscription: &str) -> Result<()> {
        Binding::parse(saved.binding, saved.workspace)?;
        composio_triggers::unsubscribe(&self.api, subscription).await
    }
    fn parse_delivery(
        &self,
        headers: &axum::http::HeaderMap,
        body: &[u8],
        secret: &str,
        now: i64,
    ) -> Result<Vec<Delivery>> {
        composio_triggers::parse(headers, body, secret, now)
    }
}

struct ComposioCall {
    api: ComposioApi,
    body: Value,
    tool: String,
    fields: BTreeMap<String, String>,
}
#[async_trait::async_trait]
impl Call for ComposioCall {
    async fn run(mut self: Box<Self>, arguments: &Value, _key: &str) -> Outcome {
        let Ok(arguments) = curated::provider_arguments(&self.fields, arguments) else {
            return Outcome::Uncertain;
        };
        self.body["arguments"] = arguments;
        let path = format!("tools/execute/{}", self.tool);
        match self.api.send(Method::POST, &path, Some(&self.body)).await {
            Ok(Reply::Ok(reply)) if reply["successful"] == true && reply["error"].is_null() => {
                let data = &reply["data"];
                Outcome::Completed(if data.to_string().len() <= MAX_RESULT {
                    json!({"data": data})
                } else {
                    json!({"message":"The result was too large to return. Ask for fewer items or a narrower search."})
                })
            }
            _ => Outcome::Uncertain,
        }
    }
}
