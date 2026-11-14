//! `AYATI_COMPUTER_PROVIDERS_FILE`: the computer sizes people choose, what each costs
//! them, and (private to operators) which provider image serves each size, with
//! what it costs us. Read on every use, so keys rotate and routes change in place.
//!
//! ```json
//! {"sizes": [{"size": "standard", "cpu": 2, "memory_gb": 4, "disk_gb": 20, "usd_per_hour": 0.17,
//!             "routes": [{"provider": "e2b", "image": "cuckoo-computer-standard", "usd_per_hour": 0.1}]}],
//!  "providers": [{"provider": "e2b", "key_file": "/run/secrets/e2b"}],
//!  "sleep_after_minutes": 5, "longest_work_minutes": 120}
//! ```
use super::{Provider, adapter};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The sizes people may choose, in this order on screens.
pub(crate) const SIZES: [&str; 3] = ["small", "standard", "large"];

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProvidersFile {
    pub sizes: Vec<Size>,
    pub providers: Vec<Entry>,
    /// A computer sleeps this long after its agent last used it.
    #[serde(default = "sleep_default")]
    pub sleep_after_minutes: u32,
    /// The longest a computer stays awake in one go.
    #[serde(default = "longest_default")]
    pub longest_work_minutes: u32,
}
fn sleep_default() -> u32 {
    5
}
fn longest_default() -> u32 {
    120
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct Size {
    pub size: String,
    pub cpu: u32,
    pub memory_gb: u32,
    pub disk_gb: u32,
    /// What people are charged per hour awake, in US dollars.
    pub usd_per_hour: f64,
    /// Where this size runs, first choice first, for new computers.
    pub routes: Vec<Route>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct Route {
    pub provider: String,
    /// The provider's name for Cuckoo's image at this size (an E2B template).
    pub image: String,
    /// What the provider charges us per hour awake.
    #[serde(default)]
    pub usd_per_hour: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    provider: String,
    /// A private file holding only the API key.
    key_file: PathBuf,
    /// The provider's control API unless given; loopback HTTP is for local checks.
    #[serde(default)]
    base_url: Option<String>,
    /// Where the provider serves commands and files unless given.
    #[serde(default)]
    computer_url: Option<String>,
}

/// One provider ready to call.
pub(crate) struct Routed {
    pub provider: &'static dyn Provider,
    pub api: String,
    pub computers: String,
    key_file: PathBuf,
}
impl Routed {
    /// The route of a workspace's own computer (B3). Its work goes through the
    /// connector relay, never a provider; anything else it is asked fails.
    pub fn connector() -> Self {
        Self {
            provider: &super::Connector,
            api: String::new(),
            computers: String::new(),
            key_file: PathBuf::new(),
        }
    }
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

fn checked_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).context("Invalid computer provider URL")?;
    ensure!(
        parsed.scheme() == "https"
            || (parsed.scheme() == "http" && parsed.host_str() == Some("127.0.0.1")),
        "Computer providers need HTTPS; loopback HTTP is for local checks"
    );
    Ok(())
}

impl ProvidersFile {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).context("Computer providers file unavailable")?;
        Self::parse_bytes(&bytes)
    }
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self> {
        let file: Self =
            serde_json::from_slice(bytes).context("Invalid computer providers file")?;
        file.check()?;
        Ok(file)
    }

    fn check(&self) -> Result<()> {
        for (index, entry) in self.providers.iter().enumerate() {
            ensure!(
                adapter(&entry.provider).is_some(),
                "Unknown computer provider {}",
                entry.provider
            );
            ensure!(
                !self.providers[..index]
                    .iter()
                    .any(|e| e.provider == entry.provider),
                "Computer provider {} is listed twice",
                entry.provider
            );
            for url in [&entry.base_url, &entry.computer_url].into_iter().flatten() {
                checked_url(url)?;
            }
        }
        ensure!(
            !self.sizes.is_empty(),
            "The computer providers file needs sizes"
        );
        for (index, size) in self.sizes.iter().enumerate() {
            ensure!(
                SIZES.contains(&size.size.as_str()),
                "Computer sizes are small, standard and large, not {}",
                size.size
            );
            ensure!(
                !self.sizes[..index].iter().any(|s| s.size == size.size),
                "Computer size {} is listed twice",
                size.size
            );
            ensure!(
                (1..=16).contains(&size.cpu)
                    && (2..=64).contains(&size.memory_gb)
                    && (5..=200).contains(&size.disk_gb),
                "Computer size {}: 1-16 CPUs, 2-64 GB memory and 5-200 GB disk",
                size.size
            );
            ensure!(
                !size.routes.is_empty() && size.routes.len() <= 4,
                "Computer size {} needs 1 to 4 routes",
                size.size
            );
            let prices = std::iter::once(size.usd_per_hour)
                .chain(size.routes.iter().map(|r| r.usd_per_hour));
            for price in prices {
                ensure!(
                    price.is_finite() && (0.0..=100.0).contains(&price),
                    "Computer size {}: prices are 0 to 100 dollars an hour",
                    size.size
                );
            }
            for route in &size.routes {
                self.entry(&route.provider)?;
                ensure!(
                    !route.image.is_empty()
                        && route.image.len() <= 120
                        && !route.image.chars().any(char::is_control),
                    "Computer size {} has a route without an image",
                    size.size
                );
            }
        }
        ensure!(
            (1..=120).contains(&self.sleep_after_minutes)
                && (5..=24 * 60).contains(&self.longest_work_minutes),
            "Computers sleep after 1-120 minutes and work at most 5 minutes to 24 hours in one go"
        );
        Ok(())
    }

    fn entry(&self, name: &str) -> Result<&Entry> {
        self.providers
            .iter()
            .find(|e| e.provider == name)
            .with_context(|| format!("{name} is in a route but not in providers"))
    }

    pub fn size(&self, name: &str) -> Option<&Size> {
        self.sizes.iter().find(|s| s.size == name)
    }

    /// The provider named on a computer's record, ready to call.
    pub fn routed(&self, name: &str) -> Result<Routed> {
        let entry = self.entry(name)?;
        let provider = adapter(&entry.provider).context("Unknown computer provider")?;
        let trim = |url: &Option<String>, default: &str| {
            url.as_deref()
                .unwrap_or(default)
                .trim_end_matches('/')
                .to_owned()
        };
        Ok(Routed {
            provider,
            api: trim(&entry.base_url, provider.api_url()),
            computers: trim(&entry.computer_url, provider.computer_url()),
            key_file: entry.key_file.clone(),
        })
    }
}
