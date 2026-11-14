//! Link previews: the title, site and description of the first web link in a
//! person's message, read from the page's own `<meta>` tags.
//!
//! Cuckoo fetches the page itself, so the fetch is fenced like a remote MCP
//! connection (`control::mcp`): http(s) on the standard ports only, no
//! credentials in the address, the host resolved once and pinned to its
//! public IPv4 addresses (so a second lookup cannot point it somewhere
//! private), each redirect checked the same way, no proxy, 5 seconds and
//! 256 KiB at most, HTML only. Pictures are never fetched or shown: a remote
//! image would tell that site who is reading. Results are cached a day
//! (`postgres/link_previews.rs`), failures an hour.
use super::*;
use anyhow::{Context, Result, ensure};
use std::time::Duration;

pub(super) fn routes() -> Router<Shared> {
    Router::new().route(
        "/api/identity/workspaces/{w}/chats/{chat}/messages/{message}/link-preview",
        get(preview),
    )
}

/// At most this many pages are fetched at once by this process.
static FETCHING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
const MAX_BYTES: usize = 256 * 1024;
const REDIRECTS: usize = 3;

async fn preview(
    State(app): State<Shared>,
    RoutePath((w, chat, message)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = super::super::identity_for_read(&app, &headers).await?;
    let Some(link) = app.store.message_link(&user, &w, &chat, &message).await? else {
        return Ok(Json(json!({"preview": null})));
    };
    if let Some(cached) = app.store.cached_preview(&link).await? {
        return Ok(Json(json!({"preview": cached})));
    }
    // Busy: say nothing rather than queue; the next reader tries again.
    let Ok(_permit) = tokio::time::timeout(Duration::from_secs(2), FETCHING.acquire()).await else {
        return Ok(Json(json!({"preview": null})));
    };
    let found = match reqwest::Url::parse(&link) {
        Ok(url) => fetch(url, true).await.unwrap_or(None),
        Err(_) => None,
    };
    app.store.save_preview(&link, found.as_ref()).await?;
    Ok(Json(json!({"preview": found})))
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct Preview {
    pub url: String,
    pub title: String,
    pub site: String,
    pub description: Option<String>,
}

/// The first http(s) address in a message's text, without trailing punctuation.
pub(crate) fn first_link(text: &str) -> Option<String> {
    let start = ["https://", "http://"]
        .iter()
        .filter_map(|scheme| text.find(scheme))
        .min()?;
    let link: String = text[start..]
        .chars()
        .take_while(|c| !c.is_whitespace() && !"<>()[]\"'`".contains(*c))
        .collect();
    let link = link.trim_end_matches(['.', ',', ';', ':', '!', '?', '*', '_']);
    (link.len() > "https://".len() && link.len() <= 2048).then(|| link.to_owned())
}

/// Fetches a page and reads its preview. `restrict` is false only in tests,
/// which serve pages from this machine.
pub(crate) async fn fetch(mut url: reqwest::Url, restrict: bool) -> Result<Option<Preview>> {
    for _ in 0..=REDIRECTS {
        ensure!(matches!(url.scheme(), "http" | "https"), "Only web pages");
        ensure!(
            url.username().is_empty() && url.password().is_none(),
            "No sign-in details in links"
        );
        let host = url.host_str().context("No host")?.to_owned();
        let port = url.port_or_known_default().context("No port")?;
        let mut client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .user_agent("CuckooLinkPreview/1.0");
        if restrict {
            ensure!(matches!(port, 80 | 443), "Only standard web ports");
            let addresses: Vec<_> = tokio::time::timeout(
                Duration::from_secs(3),
                tokio::net::lookup_host((host.as_str(), port)),
            )
            .await
            .context("Lookup timed out")??
            .filter(|address| crate::identity::control::mcp::public_ip(address.ip()))
            .collect();
            ensure!(!addresses.is_empty(), "Not a public address");
            client = client.resolve_to_addrs(&host, &addresses);
        }
        let mut response = client.build()?.get(url.clone()).send().await?;
        if response.status().is_redirection() {
            let next = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .context("Redirect without a place")?;
            url = url.join(next)?;
            continue;
        }
        if !response.status().is_success() {
            return Ok(None);
        }
        let html = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|kind| kind.to_ascii_lowercase().starts_with("text/html"));
        if !html {
            return Ok(None);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            body.extend_from_slice(&chunk);
            if body.len() >= MAX_BYTES {
                body.truncate(MAX_BYTES);
                break;
            }
        }
        return Ok(read_meta(&String::from_utf8_lossy(&body), &url));
    }
    anyhow::bail!("Too many redirects")
}

/// The preview a page describes for itself: Open Graph first, then Twitter,
/// then the plain description and `<title>`. None without a title.
pub(crate) fn read_meta(html: &str, url: &reqwest::Url) -> Option<Preview> {
    let head = &html[..html
        .to_ascii_lowercase()
        .find("</head>")
        .unwrap_or(html.len())];
    let mut meta = std::collections::HashMap::<String, String>::new();
    let lower = head.to_ascii_lowercase();
    let mut at = 0;
    while let Some(found) = lower[at..].find("<meta") {
        let start = at + found;
        let end = lower[start..].find('>').map_or(head.len(), |e| start + e);
        let attributes = attributes(&head[start + 5..end]);
        let key = attributes
            .get("property")
            .or_else(|| attributes.get("name"))
            .map(|k| k.to_ascii_lowercase());
        if let (Some(key), Some(content)) = (key, attributes.get("content")) {
            meta.entry(key).or_insert_with(|| content.clone());
        }
        at = end;
    }
    let pick = |keys: &[&str]| keys.iter().find_map(|k| meta.get(*k)).map(|v| clean(v));
    let title = pick(&["og:title", "twitter:title"])
        .or_else(|| {
            let open = lower.find("<title")?;
            let from = open + lower[open..].find('>')? + 1;
            let to = from + lower[from..].find("</title")?;
            Some(clean(&head[from..to]))
        })
        .filter(|t| !t.is_empty())?;
    let host = url.host_str().unwrap_or_default();
    let site = pick(&["og:site_name"])
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| host.trim_start_matches("www.").to_owned());
    let description =
        pick(&["og:description", "twitter:description", "description"]).filter(|d| !d.is_empty());
    Some(Preview {
        url: url.to_string(),
        title: cut(&title, 200),
        site: cut(&site, 80),
        description: description.map(|d| cut(&d, 300)),
    })
}

/// `name="value"`, `name='value'` and `name=value` pairs of one tag.
fn attributes(tag: &str) -> std::collections::HashMap<String, String> {
    let mut found = std::collections::HashMap::new();
    let mut rest = tag.trim_start();
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq]
            .trim()
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let value = rest[eq + 1..].trim_start();
        let (text, after) = match value.chars().next() {
            Some(quote @ ('"' | '\'')) => match value[1..].find(quote) {
                Some(close) => (&value[1..1 + close], &value[close + 2..]),
                None => (&value[1..], ""),
            },
            _ => {
                let end = value
                    .find(|c: char| c.is_whitespace() || c == '/')
                    .unwrap_or(value.len());
                (&value[..end], &value[end..])
            }
        };
        if !name.is_empty() {
            found.entry(name).or_insert_with(|| text.to_owned());
        }
        rest = after;
    }
    found
}

/// Entities decoded, tags dropped, whitespace on one line.
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest
            .char_indices()
            .take(10)
            .find(|(_, c)| *c == ';')
            .map(|(i, _)| i)
        else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    format!(
        "{}…",
        text.chars().take(max - 1).collect::<String>().trim_end()
    )
}

#[cfg(test)]
#[path = "link_preview_tests.rs"]
mod tests;
