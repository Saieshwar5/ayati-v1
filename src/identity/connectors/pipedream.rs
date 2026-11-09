//! Pipedream Connect as a connector provider. Pipedream hosts the sign-in page
//! (Connect Link) and keeps the tokens; Cuckoo offers only the actions in its
//! own catalog, reduced to the reviewed plain props, and pins the component
//! version each one was reviewed at. Every call first checks that the account is
//! still healthy and belongs to the workspace's external user, and that the
//! pinned component still takes the same props. The agent never sees Pipedream.
pub(crate) use super::pipedream_api::Settings;
use super::{
    Account, App, Call, Capabilities, Connect, ConnectorProvider, Delivery, Outcome, Refusal,
    Saved, Subscription,
    curated::{self, Action, CatalogApp},
    fingerprint,
    pipedream_accounts::{Binding, mailbox, user},
    pipedream_api::{PipedreamApi, Reply, id},
    pipedream_props::{self, Component},
};
use crate::identity::control::model::Operation;
use anyhow::{Context, Result, ensure};
use reqwest::Method;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const NAME: &str = "pipedream";
/// Results larger than this are summarized instead of returned to the agent.
const MAX_RESULT: usize = 64 * 1024;
/// Seconds a sign-in page stays usable, from Cuckoo's side and Pipedream's.
const SIGN_IN_SECONDS: i64 = 15 * 60;

pub(super) struct Pipedream {
    pub(super) api: PipedreamApi,
}

impl Pipedream {
    pub(super) fn new(settings: Settings) -> Result<Self> {
        Ok(Self {
            api: PipedreamApi::new(&settings)?,
        })
    }

    fn app(id: &str) -> Result<(&'static CatalogApp, &'static str)> {
        let app = curated::app(id).context("This connector is not offered on this server")?;
        let slug = app
            .providers
            .get(NAME)
            .context("Pipedream does not serve this app")?;
        Ok((app, slug.as_str()))
    }

    /// The component `key` of `kind` (`actions` or `triggers`), at `version` or
    /// the latest, as Cuckoo can offer it. `None` when Pipedream has no such
    /// component or Cuckoo can't offer it (see `pipedream_props`).
    pub(super) async fn component(
        &self,
        kind: &str,
        key: &str,
        version: Option<&str>,
        app: &str,
    ) -> Result<Option<Component>> {
        let query: Vec<(&str, &str)> = version.map(|v| ("version", v)).into_iter().collect();
        match self
            .api
            .send(Method::GET, &format!("{kind}/{}", id(key)?), &query, None)
            .await?
        {
            Reply::Ok(answer) => Ok(pipedream_props::read(&answer["data"], key, version, app).ok()),
            Reply::Status(404) => Ok(None),
            Reply::Status(code) => {
                anyhow::bail!("Pipedream refused the component request (HTTP {code})")
            }
        }
    }

    /// Cuckoo's actions for `app`, each reviewed against Pipedream's current
    /// component. Actions Pipedream can't offer with plain, reviewed props are
    /// left out.
    async fn operations(&self, app: &CatalogApp, slug: &str) -> Result<Vec<Operation>> {
        let mut operations = vec![];
        for action in &app.actions {
            let Some(binding) = action.providers.get(NAME) else {
                continue;
            };
            let Some(component) = self.component("actions", &binding.tool, None, slug).await?
            else {
                continue;
            };
            let Some(schema) = reviewed(action, &component) else {
                continue;
            };
            let operation = Operation {
                id: action.id.clone(),
                title: action.title.clone(),
                tool: binding.tool.clone(),
                effect: action.effect.clone(),
                schema,
                version: Some(component.version),
            };
            if operation.validate().is_ok() {
                operations.push(operation);
            }
        }
        Ok(operations)
    }
}

/// The schema Cuckoo offers for `action` with this component, if it can: only
/// reviewed plain props, and a read must not be a component that says it writes.
fn reviewed(action: &Action, component: &Component) -> Option<Value> {
    if action.effect == "read" && component.read_only == Some(false) {
        return None;
    }
    curated::project(action, action.providers.get(NAME)?, &component.schema)
}

#[async_trait::async_trait]
impl ConnectorProvider for Pipedream {
    fn name(&self) -> &'static str {
        NAME
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
        let (_, slug) = Self::app(app)?;
        let user = user(workspace);
        let started = crate::identity::user::now();
        let known = self.known(&user, slug).await?;
        let token = self
            .api
            .request(
                Method::POST,
                "tokens",
                &[],
                Some(
                    &json!({"external_user_id":user,"app_id":slug,"expires_in":SIGN_IN_SECONDS,
                    "success_redirect_uri":callback,"error_redirect_uri":callback}),
                ),
            )
            .await?;
        let mut url = reqwest::Url::parse(
            token["connect_link_url"]
                .as_str()
                .context("Pipedream omitted the sign-in page")?,
        )
        .context("Pipedream returned an invalid sign-in page")?;
        ensure!(
            url.scheme() == "https"
                || (self.api.is_fixture() && url.host_str() == Some("127.0.0.1")),
            "Pipedream returned an insecure sign-in page"
        );
        // Connect Link opens the app named here (Pipedream's Connect Link docs).
        if !url.query_pairs().any(|(key, _)| key == "app") {
            url.query_pairs_mut().append_pair("app", slug);
        }
        Ok(Connect::SignIn {
            binding: json!({"user":user,"app":slug,"started":started,"known":known}),
            url: url.into(),
            expires: started + SIGN_IN_SECONDS,
        })
    }

    async fn finish(&self, workspace: &str, app: &str, value: &Value) -> Result<Account> {
        let (catalog, slug) = Self::app(app)?;
        let user = user(workspace);
        ensure!(
            value["user"] == user.as_str() && value["app"] == slug,
            Refusal::Changed
        );
        ensure!(value["started"].is_i64(), Refusal::Changed);
        let known = value["known"].as_object().context(Refusal::Changed)?;
        // Pipedream may still be saving the account when the person returns.
        let mut attempt = 0;
        let account = loop {
            if let Some(account) = self.signed_in(&user, slug, known).await? {
                break account;
            }
            ensure!(
                attempt < 3,
                "The sign-in did not finish. Try connecting again."
            );
            attempt += 1;
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        };
        let operations = self.operations(catalog, slug).await?;
        ensure!(
            !operations.is_empty(),
            "Pipedream offers none of Cuckoo's reviewed actions for {}",
            catalog.name
        );
        let binding = json!({"account":account.id,"app":slug,"user":user});
        let definition = json!({"id":catalog.id,"name":catalog.name,"account":account.name,"operations":operations});
        Ok(Account {
            fingerprint: fingerprint(NAME, &definition, Some(&binding)),
            definition,
            binding: Some(binding),
            identity: mailbox(account.name.as_deref()),
        })
    }

    async fn disconnect(&self, workspace: &str, _app: &str, value: Option<&Value>) -> Result<()> {
        // A sign-in that never finished names no account: there is nothing to remove.
        if value.is_some_and(|v| v.get("account").is_none() && v.get("started").is_some()) {
            return Ok(());
        }
        self.remove_account(&Binding::parse(value, workspace)?)
            .await
    }

    async fn prepare(&self, saved: &Saved<'_>, operation: &Operation) -> Result<Box<dyn Call>> {
        let binding = Binding::parse(saved.binding, saved.workspace)?;
        let (app, slug) = Self::app(saved.server).map_err(|_| Refusal::Changed)?;
        ensure!(binding.app == slug, Refusal::Changed);
        let action = app
            .actions
            .iter()
            .find(|a| a.id == operation.id)
            .context(Refusal::Changed)?;
        let reviewed_binding = action.providers.get(NAME).context(Refusal::Changed)?;
        ensure!(reviewed_binding.tool == operation.tool, Refusal::Changed);
        let version = operation.version.as_deref().context(Refusal::Changed)?;
        if !self.healthy(&binding).await? {
            return Err(Refusal::NeedsReconnect.into());
        }
        let component = self
            .component("actions", &operation.tool, Some(version), slug)
            .await?
            .context(Refusal::Changed)?;
        ensure!(
            reviewed(action, &component).as_ref() == Some(&operation.schema),
            Refusal::Changed
        );
        Ok(Box::new(PipedreamCall {
            api: self.api.clone(),
            body: json!({"id":operation.tool,"version":version,"external_user_id":binding.user}),
            app_prop: component.app_prop,
            account: binding.account,
            fields: reviewed_binding.fields.clone(),
            lists: component.lists,
        }))
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            sign_in: true,
            revoke: true,
            list_fields: false,
            // Running an action takes no idempotency key.
            idempotency_key: false,
            find_outcome: false,
            triggers: true,
        }
    }

    /// The latest component version's props as JSON Schema (unsupported props
    /// marked), the same shape the reviewed schema was projected from. Calls
    /// keep running the pinned version until the account is reviewed again.
    async fn describe(&self, saved: &Saved<'_>, operation: &Operation) -> Result<Value> {
        let binding = Binding::parse(saved.binding, saved.workspace)?;
        let component = self
            .component("actions", &operation.tool, None, &binding.app)
            .await?
            .context("Pipedream no longer offers this action as Cuckoo reviewed it")?;
        Ok(component.schema)
    }

    async fn subscribe(
        &self,
        saved: &Saved<'_>,
        trigger: &str,
        config: &Value,
        webhook: &str,
    ) -> Result<Subscription> {
        self.deploy(saved, trigger, config, webhook).await
    }

    async fn unsubscribe(&self, saved: &Saved<'_>, subscription: &str) -> Result<()> {
        self.remove_trigger(saved.workspace, subscription).await
    }

    fn parse_delivery(
        &self,
        headers: &axum::http::HeaderMap,
        body: &[u8],
        secret: &str,
        now: i64,
    ) -> Result<Vec<Delivery>> {
        super::pipedream_events::parse(headers, body, secret, now)
    }
}

struct PipedreamCall {
    api: PipedreamApi,
    body: Value,
    app_prop: String,
    account: String,
    fields: BTreeMap<String, String>,
    lists: BTreeSet<String>,
}
#[async_trait::async_trait]
impl Call for PipedreamCall {
    async fn run(mut self: Box<Self>, arguments: &Value, _key: &str) -> Outcome {
        let Ok(Value::Object(mut props)) = curated::provider_arguments(&self.fields, arguments)
        else {
            return Outcome::Uncertain;
        };
        for (name, value) in props.iter_mut() {
            if self.lists.contains(name) && value.is_string() {
                *value = json!([value.take()]);
            }
        }
        // Set last, so no mapped field can name another account.
        props.insert(
            self.app_prop.clone(),
            json!({"authProvisionId": self.account}),
        );
        self.body["configured_props"] = Value::Object(props);
        match self
            .api
            .send(Method::POST, "actions/run", &[], Some(&self.body))
            .await
        {
            Ok(Reply::Ok(reply)) if confirmed(&reply) => {
                let mut result = json!({"data": reply["ret"]});
                if let Some(summary) = reply["exports"]["$summary"].as_str() {
                    result["summary"] = json!(summary.chars().take(500).collect::<String>());
                }
                Outcome::Completed(if result.to_string().len() <= MAX_RESULT {
                    result
                } else {
                    json!({"message":"The result was too large to return. Ask for fewer items or a narrower search."})
                })
            }
            // A timeout, an error, an asynchronous start (202) or anything unclear.
            _ => Outcome::Uncertain,
        }
    }
}

/// A run Pipedream reports as finished without an error: no `error` object and
/// no observation of a thrown error.
fn confirmed(reply: &Value) -> bool {
    reply.is_object()
        && reply.get("error").is_none_or(Value::is_null)
        && reply["os"]
            .as_array()
            .is_none_or(|os| os.iter().all(|o| o.get("err").is_none_or(Value::is_null)))
}
