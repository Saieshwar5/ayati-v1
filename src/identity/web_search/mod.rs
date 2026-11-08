//! Web search for agents: Cuckoo's own `web_search` and `read_web_page` tools are
//! answered by outside search services. Agents and people never see which one; the
//! controller holds the keys, chooses the service and falls back in the operator's
//! order. Adding a service is one file implementing [`Provider`] and one line in
//! [`adapter`].
mod exa;
mod parallel;
mod perplexity;
mod providers_file;
mod tavily;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::Value;

pub(crate) use providers_file::ProvidersFile;

/// Results asked of a service for one search.
pub(crate) const RESULTS: usize = 8;
/// Characters of each result's passage kept for the agent.
pub(crate) const PASSAGE_CHARS: usize = 700;
/// Characters of a page kept; the agent reads them a part at a time.
pub(crate) const PAGE_CHARS: usize = 60_000;
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// How recent results must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Recent {
    Day,
    Week,
    Month,
    Year,
}
impl Recent {
    pub(crate) fn days(self) -> i64 {
        match self {
            Self::Day => 1,
            Self::Week => 7,
            Self::Month => 31,
            Self::Year => 366,
        }
    }
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }
}

/// One search, in Cuckoo's terms; each provider translates it.
#[derive(Clone, Debug)]
pub(crate) struct Query {
    pub text: String,
    pub recent: Option<Recent>,
    /// Only these sites (host names), when given.
    pub sites: Vec<String>,
}

/// One search result.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Found {
    pub title: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    pub passage: String,
}

/// A page's main text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Page {
    pub url: String,
    pub title: String,
    pub text: String,
}

/// Where and how to call a provider, from the providers file.
pub(crate) struct Call<'a> {
    pub base: &'a str,
    pub key: &'a str,
}

/// What every search service implements. Requests and answers use Cuckoo's own
/// types, so tools, records and screens never depend on a provider.
#[async_trait::async_trait]
pub(crate) trait Provider: Send + Sync {
    /// The name in the providers file and on records; never shown in workspaces.
    fn name(&self) -> &'static str;
    /// The provider's public API, unless the providers file names another.
    fn base_url(&self) -> &'static str;
    /// Whether it can return a page's text (some services only search).
    fn reads_pages(&self) -> bool;
    async fn search(&self, call: &Call<'_>, query: &Query) -> Result<Vec<Found>>;
    async fn read(&self, _call: &Call<'_>, _url: &str) -> Result<Page> {
        bail!("{} can't read pages", self.name())
    }
}

/// Every provider this Cuckoo version supports, by its providers-file name.
pub(crate) fn adapter(name: &str) -> Option<&'static dyn Provider> {
    let found: &'static dyn Provider = match name {
        "tavily" => &tavily::Tavily,
        "exa" => &exa::Exa,
        "perplexity" => &perplexity::Perplexity,
        "parallel" => &parallel::Parallel,
        _ => return None,
    };
    Some(found)
}

/// An answer and what it cost, with the provider that gave it (for records only).
#[derive(Debug)]
pub(crate) struct Answered<T> {
    pub value: T,
    pub provider: &'static str,
    pub cost_usd: f64,
}

/// The controller's web search, when a providers file is configured.
#[derive(Clone)]
pub(crate) struct WebSearch {
    file: std::path::PathBuf,
}
impl WebSearch {
    pub(crate) fn from(config: &crate::config::Controller) -> Option<Self> {
        config.web_search_providers_file.clone().map(Self::new)
    }
    pub(crate) fn new(file: std::path::PathBuf) -> Self {
        Self { file }
    }
    /// Checks the file now, so a broken one stops the controller at start.
    pub(crate) fn check(&self) -> Result<()> {
        ProvidersFile::read(&self.file).map(|_| ())
    }
    /// Search with the first provider in the search order that answers.
    pub(crate) async fn search(&self, query: &Query) -> Result<Answered<Vec<Found>>> {
        let file = ProvidersFile::read(&self.file)?;
        let mut last = None;
        for routed in file.route(&file.search) {
            let answer = async {
                let key = routed.key()?;
                let call = Call {
                    base: &routed.base,
                    key: &key,
                };
                routed.provider.search(&call, query).await
            };
            match answer.await {
                Ok(mut found) => {
                    found.truncate(RESULTS);
                    return Ok(Answered {
                        value: found,
                        provider: routed.provider.name(),
                        cost_usd: routed.search_usd,
                    });
                }
                Err(error) => last = Some(error.context(routed.provider.name())),
            }
        }
        Err(unavailable(last))
    }
    /// Read a page with the first provider in the reading order that answers.
    pub(crate) async fn read(&self, url: &str) -> Result<Answered<Page>> {
        public_url(url)?;
        let file = ProvidersFile::read(&self.file)?;
        let mut last = None;
        for routed in file.route(&file.read) {
            let answer = async {
                let key = routed.key()?;
                let call = Call {
                    base: &routed.base,
                    key: &key,
                };
                routed.provider.read(&call, url).await
            };
            let name = routed.provider.name();
            match answer.await {
                Ok(mut page) if !page.text.trim().is_empty() => {
                    page.text = clip(&page.text, PAGE_CHARS);
                    return Ok(Answered {
                        value: page,
                        provider: name,
                        cost_usd: routed.read_usd,
                    });
                }
                Ok(_) => last = Some(anyhow::anyhow!("{name}: the page had no text")),
                Err(error) => last = Some(error.context(name)),
            }
        }
        Err(unavailable(last))
    }
}

fn unavailable(last: Option<anyhow::Error>) -> anyhow::Error {
    match last {
        Some(error) => error.context("No web search service answered"),
        None => anyhow::anyhow!("No web search service is set up for this"),
    }
}

/// Only public web addresses: the service fetches them, but an address that
/// names this machine or a private network is refused before anything is sent.
pub(crate) fn public_url(text: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(text).context("Not a web address")?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "Only http and https pages"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "Web addresses with sign-in details are refused"
    );
    let host = url
        .host_str()
        .context("A web address needs a host")?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    let private = match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
                || ip.to_ipv4_mapped().is_some()
        }
        Err(_) => {
            !host.contains('.')
                || [".localhost", ".local", ".internal", ".lan", ".home.arpa"]
                    .iter()
                    .any(|end| host.ends_with(end))
        }
    };
    ensure!(!private, "Private and local addresses are refused");
    Ok(url)
}

/// At most `limit` characters, cut on a character boundary.
pub(crate) fn clip(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((at, _)) => text[..at].to_owned(),
        None => text.to_owned(),
    }
}

/// POST JSON with a timeout; a refusal or a non-JSON answer is an error naming the status.
async fn post(url: &str, headers: &[(&str, String)], body: &Value) -> Result<Value> {
    let mut request = client().post(url).json(body).timeout(TIMEOUT);
    for (name, value) in headers {
        request = request.header(*name, value.as_str());
    }
    let response = request.send().await.context("could not be reached")?;
    let status = response.status();
    ensure!(status.is_success(), "refused the request ({status})");
    response
        .json()
        .await
        .context("answered with something other than JSON")
}

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("web search client")
    })
}

/// A text field, or empty.
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().trim().to_owned()
}
/// A result with a title, address and passage; results without an address are dropped.
fn found(title: String, url: String, date: Option<String>, passage: &str) -> Option<Found> {
    (!url.is_empty()).then(|| Found {
        title: if title.is_empty() { url.clone() } else { title },
        url,
        date: date.filter(|d| !d.is_empty()),
        passage: clip(passage.trim(), PASSAGE_CHARS),
    })
}
