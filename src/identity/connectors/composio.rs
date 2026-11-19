//! Composio as a connector provider. Composio hosts the sign-in pages and keeps
//! the tokens; Cuckoo offers only the actions in its own catalog, reduced to the
//! reviewed fields, and pins the Composio tool version each one was reviewed at.
//! Every call first checks that the account is still active and the pinned tool
//! still means the same thing; the agent never sees Composio.
use super::{
    Account, App, Call, Capabilities, Connect, ConnectorProvider, Delivery, LibraryPick, Refusal,
    Saved, Subscription,
    added::AddedApp,
    composio_api::{self, ComposioApi, Reply},
    composio_call::ComposioCall,
    composio_library, composio_triggers,
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
    pub(super) api: ComposioApi,
    pub(super) auth_configs: BTreeMap<String, String>,
    /// Apps this workspace's admins added from the library (schema 135), when
    /// the provider was built for a workspace.
    added: Vec<CatalogApp>,
}

/// Composio's identity for a connected account, saved on the connection.
pub(super) struct Binding {
    pub(super) account: String,
    pub(super) auth_config: String,
    pub(super) user: String,
    /// The toolkit of an app an admin added; built-in apps name theirs in the catalog.
    toolkit: Option<String>,
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
            toolkit: match value.get("toolkit") {
                Some(_) => Some(text("toolkit")?),
                None => None,
            },
        };
        ensure!(binding.user == user(workspace), Refusal::Changed);
        Ok(binding)
    }
}

/// Composio keeps accounts per "user"; each Cuckoo workspace is one.
fn user(workspace: &str) -> String {
    format!("cuckoo-{workspace}")
}

pub(super) enum State {
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
            added: vec![],
        })
    }

    /// The same provider, also offering the apps a workspace's admins added.
    pub(super) fn with_added(mut self, apps: Vec<CatalogApp>) -> Self {
        self.added = apps
            .into_iter()
            .filter(|app| app.providers.contains_key(NAME) && curated::app(&app.id).is_none())
            .collect();
        self
    }

    /// A built-in app, or one this workspace's admins added, with its toolkit.
    fn app(&self, id: &str) -> Result<(&CatalogApp, &str)> {
        let app = curated::app(id)
            .or_else(|| self.added.iter().find(|a| a.id == id))
            .context("This connector is not offered on this server")?;
        let toolkit = app
            .providers
            .get(NAME)
            .context("Composio does not serve this app")?;
        Ok((app, toolkit.as_str()))
    }

    /// The toolkit of a saved connection: the catalog's for a built-in app, the
    /// one saved on the connection for an app an admin added.
    fn toolkit(binding: &Binding, app: &str) -> Result<String, Refusal> {
        match curated::app(app) {
            Some(catalog) => catalog.providers.get(NAME).cloned(),
            None => binding.toolkit.clone(),
        }
        .ok_or(Refusal::Changed)
    }

    /// The action and Composio binding `operation` was reviewed against: the
    /// catalog's for a built-in app, while this release still maps it to the
    /// same tool; the saved operation's own for an app an admin added.
    fn reviewed(app: &str, operation: &Operation) -> Option<(curated::Action, curated::Binding)> {
        let Some(catalog) = curated::app(app) else {
            return Some(super::added::reviewed(operation));
        };
        let action = catalog.actions.iter().find(|a| a.id == operation.id)?;
        let binding = action.providers.get(NAME)?;
        (binding.tool == operation.tool).then(|| (action.clone(), binding.clone()))
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
            library: true,
            ..Capabilities::default()
        }
    }
    async fn library(&self, query: &str) -> Result<Value> {
        composio_library::search(&self.api, query).await
    }
    async fn library_actions(&self, app: &str, query: &str, page: Option<&str>) -> Result<Value> {
        composio_library::actions(&self.api, app, query, page).await
    }
    async fn library_app(&self, app: &str, picks: &[LibraryPick]) -> Result<AddedApp> {
        composio_library::build(&self.api, app, picks).await
    }
    fn apps(&self, _workspace: &str) -> Result<Vec<App>> {
        Ok(curated::catalog()
            .apps
            .iter()
            .chain(&self.added)
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
        let (_, toolkit) = self.app(app)?;
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
        let mut binding =
            json!({"account":account,"auth_config":auth_config,"user":user(workspace)});
        if curated::app(app).is_none() {
            binding["toolkit"] = json!(toolkit);
        }
        Ok(Connect::SignIn {
            binding,
            url: url.into(),
            expires: crate::identity::user::now() + SIGN_IN_SECONDS,
        })
    }

    async fn finish(&self, workspace: &str, app: &str, value: &Value) -> Result<Account> {
        let (catalog, toolkit) = self.app(app)?;
        let binding = Binding::parse(Some(value), workspace)?;
        ensure!(
            binding
                .toolkit
                .as_deref()
                .is_none_or(|saved| saved == toolkit),
            Refusal::Changed
        );
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
        let reduce = |tool: &Value| curated::project(&action, &reviewed, &tool["input_parameters"]);
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
        let toolkit = Self::toolkit(&binding, saved.server)?;
        let (action, reviewed) =
            Self::reviewed(saved.server, operation).context(Refusal::Changed)?;
        match self.state(&binding, &toolkit).await? {
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
            curated::project(&action, &reviewed, &tool["input_parameters"]).as_ref()
                == Some(&operation.schema),
            Refusal::Changed
        );
        Ok(Box::new(ComposioCall {
            api: self.api.clone(),
            body: json!({"connected_account_id":binding.account,"user_id":binding.user,"version":operation.version}),
            tool: operation.tool.clone(),
            fields: reviewed.fields,
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
        // Only built-in apps have events.
        let app = curated::app(saved.server).ok_or(Refusal::Changed)?;
        let toolkit = Self::toolkit(&binding, saved.server)?;
        ensure!(
            app.triggers
                .iter()
                .any(|t| t.providers.get(NAME).is_some_and(|b| b.tool == trigger)),
            Refusal::Changed
        );
        match self.state(&binding, &toolkit).await? {
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
