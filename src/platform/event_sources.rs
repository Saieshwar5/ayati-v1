use super::{Platform, identifier};
use anyhow::{Context, Result, ensure};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewEventSource {
    pub slug: String,
    pub version: String,
    pub config: Value,
}
impl Platform {
    pub async fn event_types(&self, id: &str) -> Result<Value> {
        let (_, toolkit, status) = self.account_for(id)?;
        ensure!(status == "ACTIVE", "Choose an active connection");
        identifier(&toolkit)?;
        let types = self
            .client()?
            .request(
                Method::GET,
                &format!("triggers_types?toolkit_slugs={toolkit}&limit=50"),
                None,
            )
            .await?;
        let types=types["items"].as_array().context("Invalid event catalog")?.iter().filter(|t|t["toolkit"]["slug"]==toolkit).map(|t|json!({"slug":t["slug"],"name":t["name"],"description":t["description"],"version":t["version"],"config":t["config"]})).collect::<Vec<_>>();
        Ok(json!(types))
    }
    pub async fn create_event_source(&self, id: &str, input: &NewEventSource) -> Result<Value> {
        identifier(&input.slug)?;
        identifier(&input.version)?;
        ensure!(
            input.version != "latest"
                && input.config.is_object()
                && input.config.to_string().len() <= 4000,
            "Choose an explicit event version and bounded configuration"
        );
        let _gate = self.gate.write().await;
        let (account, toolkit, status) = self.account_for(id)?;
        ensure!(
            status == "ACTIVE"
                && self
                    .client()?
                    .account(&account, &self.user_id, &toolkit)
                    .await?
                    == "ACTIVE",
            "Account is not active"
        );
        let mut url = reqwest::Url::parse("https://backend.composio.dev/api/v3.1/")?
            .join(&format!("triggers_types/{}", input.slug))?;
        url.query_pairs_mut()
            .append_pair(&format!("toolkit_versions[{toolkit}]"), &input.version);
        let path = format!("triggers_types/{}?{}", input.slug, url.query().unwrap());
        let definition = self.client()?.request(Method::GET, &path, None).await?;
        ensure!(
            definition["toolkit"]["slug"] == toolkit
                && definition["slug"] == input.slug
                && definition["version"] == input.version,
            "Event does not match the selected connection/version"
        );
        let source=self.client()?.request(Method::POST,&format!("trigger_instances/{}/upsert",input.slug),Some(&json!({"connected_account_id":account,"user_id":self.user_id,"toolkit_versions":{toolkit:input.version},"trigger_config":input.config}))).await?;
        let provider = source["trigger_id"]
            .as_str()
            .context("Event subscription was not acknowledged; inspect Composio before retrying")?;
        identifier(provider)?;
        Ok(json!({"id":provider,"slug":input.slug,"version":input.version}))
    }
}
