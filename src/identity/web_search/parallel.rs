//! Parallel (api.parallel.ai): `POST /v1beta/search` returns results with excerpts
//! for an objective; `POST /v1beta/extract` returns a page's full text. Key in
//! `x-api-key`; both endpoints are in beta and named by `parallel-beta`.
use super::{Call, Found, Page, Provider, Query, RESULTS, found, post, text};
use anyhow::{Context, Result};
use serde_json::{Value, json};

pub(crate) struct Parallel;
const BETA: &str = "search-extract-2025-10-10";

#[async_trait::async_trait]
impl Provider for Parallel {
    fn name(&self) -> &'static str {
        "parallel"
    }
    fn base_url(&self) -> &'static str {
        "https://api.parallel.ai"
    }
    fn reads_pages(&self) -> bool {
        true
    }
    async fn search(&self, call: &Call<'_>, query: &Query) -> Result<Vec<Found>> {
        let mut objective = query.text.clone();
        if let Some(recent) = query.recent {
            objective.push_str(&format!(" (published in the last {})", recent.word()));
        }
        let mut body = json!({
            "objective": objective, "search_queries": [query.text], "max_results": RESULTS,
            "excerpts": {"max_chars_per_result": super::PASSAGE_CHARS},
        });
        if !query.sites.is_empty() {
            body["source_policy"] = json!({"include_domains": query.sites});
        }
        let answer = post(&format!("{}/v1beta/search", call.base), &auth(call), &body).await?;
        Ok(answer["results"]
            .as_array()
            .context("Parallel's answer has no results")?
            .iter()
            .filter_map(|r| {
                let date = r["publish_date"].as_str().map(str::to_owned);
                found(text(r, "title"), text(r, "url"), date, &excerpts(r))
            })
            .collect())
    }
    async fn read(&self, call: &Call<'_>, url: &str) -> Result<Page> {
        let body = json!({"urls": [url], "excerpts": false, "full_content": true});
        let answer = post(&format!("{}/v1beta/extract", call.base), &auth(call), &body).await?;
        let page = answer["results"]
            .as_array()
            .and_then(|r| r.first())
            .context("Parallel could not read the page")?;
        let full = text(page, "full_content");
        Ok(Page {
            url: url.to_owned(),
            title: text(page, "title"),
            text: if full.is_empty() {
                excerpts(page)
            } else {
                full
            },
        })
    }
}

fn excerpts(result: &Value) -> String {
    result["excerpts"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" … ")
        })
        .unwrap_or_default()
}

fn auth(call: &Call<'_>) -> [(&'static str, String); 2] {
    [
        ("x-api-key", call.key.to_owned()),
        ("parallel-beta", BETA.to_owned()),
    ]
}
