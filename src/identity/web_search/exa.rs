//! Exa (api.exa.ai): `POST /search` with highlights gives results and passages;
//! `POST /contents` gives a page's text. Key in `x-api-key`.
use super::{Call, Found, Page, Provider, Query, RESULTS, found, post, text};
use anyhow::{Context, Result};
use serde_json::{Value, json};

pub(crate) struct Exa;

#[async_trait::async_trait]
impl Provider for Exa {
    fn name(&self) -> &'static str {
        "exa"
    }
    fn base_url(&self) -> &'static str {
        "https://api.exa.ai"
    }
    fn reads_pages(&self) -> bool {
        true
    }
    async fn search(&self, call: &Call<'_>, query: &Query) -> Result<Vec<Found>> {
        let mut body = json!({
            "query": query.text, "type": "auto", "numResults": RESULTS,
            "contents": {"highlights": {"numSentences": 3, "highlightsPerUrl": 2}},
        });
        if let Some(recent) = query.recent {
            body["startPublishedDate"] =
                json!(format!("{}T00:00:00.000Z", days_ago(recent.days())));
        }
        if !query.sites.is_empty() {
            body["includeDomains"] = json!(query.sites);
        }
        let answer = post(&format!("{}/search", call.base), &auth(call), &body).await?;
        Ok(answer["results"]
            .as_array()
            .context("Exa's answer has no results")?
            .iter()
            .filter_map(|r| {
                let passage = match r["highlights"].as_array() {
                    Some(parts) if !parts.is_empty() => parts
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" … "),
                    _ => text(r, "text"),
                };
                let date = r["publishedDate"]
                    .as_str()
                    .map(|d| d.chars().take(10).collect());
                found(text(r, "title"), text(r, "url"), date, &passage)
            })
            .collect())
    }
    async fn read(&self, call: &Call<'_>, url: &str) -> Result<Page> {
        let body = json!({"urls": [url], "text": true});
        let answer = post(&format!("{}/contents", call.base), &auth(call), &body).await?;
        let page = answer["results"]
            .as_array()
            .and_then(|r| r.first())
            .context("Exa could not read the page")?;
        Ok(Page {
            url: url.to_owned(),
            title: text(page, "title"),
            text: text(page, "text"),
        })
    }
}

fn auth(call: &Call<'_>) -> [(&'static str, String); 1] {
    [("x-api-key", call.key.to_owned())]
}

/// The UTC date `days` ago as YYYY-MM-DD (days-to-civil, H. Hinnant's algorithm).
pub(super) fn days_ago(days: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let z = now / 86_400 - days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
