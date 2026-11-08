//! Tavily (api.tavily.com): `POST /search` returns results with a relevant passage;
//! `POST /extract` returns a page's text as Markdown. Bearer key.
use super::{Call, Found, Page, Provider, Query, RESULTS, found, post, text};
use anyhow::{Context, Result};
use serde_json::{Value, json};

pub(crate) struct Tavily;

#[async_trait::async_trait]
impl Provider for Tavily {
    fn name(&self) -> &'static str {
        "tavily"
    }
    fn base_url(&self) -> &'static str {
        "https://api.tavily.com"
    }
    fn reads_pages(&self) -> bool {
        true
    }
    async fn search(&self, call: &Call<'_>, query: &Query) -> Result<Vec<Found>> {
        let mut body = json!({
            "query": query.text, "search_depth": "basic", "max_results": RESULTS,
            "include_answer": false, "include_raw_content": false,
        });
        if let Some(recent) = query.recent {
            body["time_range"] = json!(recent.word());
        }
        if !query.sites.is_empty() {
            body["include_domains"] = json!(query.sites);
        }
        let answer = post(&format!("{}/search", call.base), &auth(call), &body).await?;
        Ok(answer["results"]
            .as_array()
            .context("Tavily's answer has no results")?
            .iter()
            .filter_map(|r| {
                let date = r["published_date"].as_str().map(str::to_owned);
                found(text(r, "title"), text(r, "url"), date, &text(r, "content"))
            })
            .collect())
    }
    async fn read(&self, call: &Call<'_>, url: &str) -> Result<Page> {
        let body = json!({"urls": [url], "extract_depth": "basic", "format": "markdown"});
        let answer = post(&format!("{}/extract", call.base), &auth(call), &body).await?;
        let page: &Value = answer["results"]
            .as_array()
            .and_then(|r| r.first())
            .context("Tavily could not read the page")?;
        Ok(Page {
            url: url.to_owned(),
            title: text(page, "title"),
            text: text(page, "raw_content"),
        })
    }
}

fn auth(call: &Call<'_>) -> [(&'static str, String); 1] {
    [("authorization", format!("Bearer {}", call.key))]
}
