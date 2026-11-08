//! `AYATI_WEB_SEARCH_PROVIDERS_FILE`: which services answer web searches, in what
//! order, with which key and price. Read on every call, so keys rotate and the
//! order changes in place. Example:
//!
//! ```json
//! {"search": ["tavily", "exa", "perplexity"], "read": ["exa", "tavily"],
//!  "providers": [{"provider": "tavily", "key_file": "/run/secrets/tavily",
//!                 "search_usd_per_1k": 8, "read_usd_per_1k": 1.6}]}
//! ```
use super::{Provider, adapter};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProvidersFile {
    /// Providers tried for a search, first choice first.
    pub search: Vec<String>,
    /// Providers tried to read a page; only ones that read pages.
    #[serde(default)]
    pub read: Vec<String>,
    pub providers: Vec<Entry>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    provider: String,
    /// A private file holding only the API key.
    key_file: PathBuf,
    /// The provider's public API unless given; loopback HTTP is for local checks.
    #[serde(default)]
    base_url: Option<String>,
    /// US dollars per 1,000 searches and per 1,000 pages read, for usage records.
    #[serde(default)]
    search_usd_per_1k: f64,
    #[serde(default)]
    read_usd_per_1k: f64,
}

/// One provider ready to be tried.
pub(crate) struct Routed {
    pub provider: &'static dyn Provider,
    pub base: String,
    key_file: PathBuf,
    /// Dollars for one search and for one page read.
    pub search_usd: f64,
    pub read_usd: f64,
}
impl Routed {
    /// The key, read now so it can be rotated in place.
    pub fn key(&self) -> Result<String> {
        let key = std::fs::read_to_string(&self.key_file)
            .with_context(|| format!("{}'s key file is unavailable", self.provider.name()))?;
        let key = key.trim();
        ensure!(
            !key.is_empty() && !key.chars().any(char::is_control),
            "{}'s key file is empty or not one line",
            self.provider.name()
        );
        Ok(key.to_owned())
    }
}

impl ProvidersFile {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).context("Web search providers file unavailable")?;
        let file: Self =
            serde_json::from_slice(&bytes).context("Invalid web search providers file")?;
        file.check()?;
        Ok(file)
    }

    #[cfg(test)]
    pub fn parse(text: &str) -> Result<Self> {
        let file: Self = serde_json::from_str(text).context("Invalid web search providers file")?;
        file.check()?;
        Ok(file)
    }

    fn check(&self) -> Result<()> {
        for (index, entry) in self.providers.iter().enumerate() {
            ensure!(
                adapter(&entry.provider).is_some(),
                "Unknown web search provider {}",
                entry.provider
            );
            ensure!(
                !self.providers[..index]
                    .iter()
                    .any(|e| e.provider == entry.provider),
                "Web search provider {} is listed twice",
                entry.provider
            );
            for price in [entry.search_usd_per_1k, entry.read_usd_per_1k] {
                ensure!(
                    price.is_finite() && (0.0..=1000.0).contains(&price),
                    "Prices for {} must be 0 to 1,000 dollars per 1,000 calls",
                    entry.provider
                );
            }
            if let Some(base) = &entry.base_url {
                let url = reqwest::Url::parse(base).context("Invalid web search provider URL")?;
                ensure!(
                    url.scheme() == "https"
                        || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1")),
                    "Web search providers need HTTPS; loopback HTTP is for local checks"
                );
            }
        }
        ensure!(
            !self.search.is_empty(),
            "The web search providers file needs a search order"
        );
        for (order, reading) in [(&self.search, false), (&self.read, true)] {
            for (index, name) in order.iter().enumerate() {
                let entry = self.entry(name)?;
                ensure!(
                    !order[..index].contains(name),
                    "{name} appears twice in one order"
                );
                if reading {
                    ensure!(
                        adapter(&entry.provider).is_some_and(|a| a.reads_pages()),
                        "{name} can't read pages, so it can't be in the read order"
                    );
                }
            }
        }
        Ok(())
    }

    fn entry(&self, name: &str) -> Result<&Entry> {
        self.providers
            .iter()
            .find(|e| e.provider == name)
            .with_context(|| format!("{name} is in an order but not in providers"))
    }

    /// The providers of one order, ready to try in turn.
    pub fn route(&self, order: &[String]) -> Vec<Routed> {
        order
            .iter()
            .filter_map(|name| {
                let entry = self.entry(name).ok()?;
                let provider = adapter(&entry.provider)?;
                Some(Routed {
                    provider,
                    base: entry
                        .base_url
                        .as_deref()
                        .unwrap_or(provider.base_url())
                        .trim_end_matches('/')
                        .to_owned(),
                    key_file: entry.key_file.clone(),
                    search_usd: entry.search_usd_per_1k / 1000.0,
                    read_usd: entry.read_usd_per_1k / 1000.0,
                })
            })
            .collect()
    }
}
