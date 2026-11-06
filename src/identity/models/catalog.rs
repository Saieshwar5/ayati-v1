//! The model catalog (MG1): the models people choose for their agents, under Cuckoo's
//! own names, and — private to us — the routes that serve each one: which provider,
//! under which of its names, in which order. People never see providers; we move a
//! model between providers by editing routes, and nobody's agent changes.
//!
//! The catalog is a JSON file (`AYATI_MODEL_CATALOG_FILE`) read on use, so edits
//! apply without a restart. Routes name providers from the model providers file
//! (`AYATI_MODEL_PROVIDERS_FILE`), which alone holds addresses and key files.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const STATUSES: [&str; 3] = ["available", "preview", "retiring"];

/// US dollars per million tokens.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    /// For prompt tokens the provider served from its cache, when it charges less.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input: Option<f64>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Supports {
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub vision: bool,
}
/// One way to serve a catalog model. Private to the operator.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// A provider from the model providers file.
    pub provider: String,
    /// The model's name at that provider.
    pub model: String,
    /// Lower is tried first; routes of one priority share traffic by weight.
    #[serde(default = "first")]
    pub priority: u8,
    #[serde(default = "even")]
    pub weight: u32,
    /// What the provider charges us.
    #[serde(default)]
    pub cost: Price,
}
fn first() -> u8 {
    1
}
fn even() -> u32 {
    100
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Tokens the model reads at once.
    #[serde(default)]
    pub context: u32,
    #[serde(default)]
    pub supports: Supports,
    /// What we charge people.
    #[serde(default)]
    pub price: Price,
    #[serde(default = "available")]
    pub status: String,
    /// For a retiring model: the catalog model people should move to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    pub routes: Vec<Route>,
}
fn available() -> String {
    "available".into()
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub models: Vec<Model>,
}

/// Where the catalog and the providers it routes to are configured.
#[derive(Clone, Debug)]
pub struct Files {
    pub catalog: PathBuf,
    pub providers: PathBuf,
}
impl Files {
    pub fn from(config: &crate::config::Controller) -> Option<Self> {
        Some(Self {
            catalog: config.model_catalog_file.clone()?,
            providers: config.model_providers_file.clone()?,
        })
    }
    /// Reads and checks the catalog against the providers file.
    pub fn load(&self) -> Result<Catalog> {
        let catalog: Catalog = serde_json::from_slice(
            &std::fs::read(&self.catalog).context("Model catalog file unavailable")?,
        )
        .context("Invalid model catalog file")?;
        catalog.validate(&providers(&self.providers)?)?;
        Ok(catalog)
    }
}

/// The provider names in the providers file.
fn providers(file: &Path) -> Result<Vec<String>> {
    let entries: Vec<Value> =
        serde_json::from_slice(&std::fs::read(file).context("Model providers file unavailable")?)
            .context("Invalid model providers file")?;
    Ok(entries
        .iter()
        .filter_map(|e| e["provider"].as_str().map(str::to_owned))
        .collect())
}

fn id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text.starts_with(|c: char| c.is_ascii_alphanumeric())
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
}
fn price(what: &str, price: &Price) -> Result<()> {
    let fine = |v: f64| v.is_finite() && (0.0..=1000.0).contains(&v);
    ensure!(
        fine(price.input) && fine(price.output) && price.cached_input.is_none_or(fine),
        "{what}: prices are 0 to 1,000 dollars per million tokens"
    );
    Ok(())
}

impl Catalog {
    pub fn validate(&self, providers: &[String]) -> Result<()> {
        ensure!(
            self.models.len() <= 200,
            "A catalog lists at most 200 models"
        );
        for (index, model) in self.models.iter().enumerate() {
            ensure!(
                id(&model.id),
                "Model id \"{}\" uses lowercase letters, numbers, . _ and -",
                model.id
            );
            ensure!(
                self.models[..index].iter().all(|m| m.id != model.id),
                "Two models are called \"{}\"",
                model.id
            );
            ensure!(
                !model.name.trim().is_empty() && model.name.chars().count() <= 80,
                "Model \"{}\" needs a name of up to 80 characters",
                model.id
            );
            ensure!(
                model.description.chars().count() <= 300 && model.family.chars().count() <= 40,
                "Model \"{}\": keep the description and family short",
                model.id
            );
            ensure!(
                model.tags.len() <= 6
                    && model
                        .tags
                        .iter()
                        .all(|t| !t.is_empty() && t.chars().count() <= 24),
                "Model \"{}\": up to 6 short tags",
                model.id
            );
            ensure!(
                STATUSES.contains(&model.status.as_str()),
                "Model \"{}\" is available, preview or retiring",
                model.id
            );
            price(&format!("Model \"{}\"", model.id), &model.price)?;
            if let Some(next) = &model.replacement {
                ensure!(
                    next != &model.id && self.models.iter().any(|m| &m.id == next),
                    "Model \"{}\" names a replacement that isn't in the catalog",
                    model.id
                );
            }
            ensure!(
                !model.routes.is_empty() && model.routes.len() <= 8,
                "Model \"{}\" has 1 to 8 routes",
                model.id
            );
            for route in &model.routes {
                ensure!(
                    providers.contains(&route.provider),
                    "Model \"{}\" routes to \"{}\", which isn't in the model providers file",
                    model.id,
                    route.provider
                );
                ensure!(
                    !route.model.is_empty()
                        && route.model.len() <= 200
                        && !route.model.chars().any(char::is_control),
                    "Model \"{}\": a route needs the provider's model name",
                    model.id
                );
                ensure!(
                    (1..=9).contains(&route.priority) && (1..=1000).contains(&route.weight),
                    "Model \"{}\": priority is 1 to 9 and weight 1 to 1,000",
                    model.id
                );
                price(
                    &format!("Model \"{}\" at {}", model.id, route.provider),
                    &route.cost,
                )?;
            }
        }
        Ok(())
    }
    pub fn find(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }
    /// A model people may choose for an agent now.
    pub fn choosable(&self, id: &str) -> Result<&Model> {
        match self.find(id) {
            None => bail!("\"{id}\" isn't one of the models Cuckoo offers"),
            Some(model) if model.status == "retiring" => bail!(
                "{} is being retired; choose {}",
                model.name,
                model
                    .replacement
                    .as_deref()
                    .and_then(|r| self.find(r))
                    .map_or("another model", |m| m.name.as_str())
            ),
            Some(model) => Ok(model),
        }
    }
}

impl Model {
    /// What people see: everything but routes and our costs.
    pub fn public(&self) -> Value {
        json!({"id":self.id,"name":self.name,"family":self.family,"description":self.description,"tags":self.tags,
            "context":self.context,"supports":self.supports,"price":self.price,"status":self.status,"replacement":self.replacement})
    }
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
