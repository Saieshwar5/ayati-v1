//! Perplexity Search API (api.perplexity.ai): `POST /search` returns ranked results
//! with snippets. It does not read whole pages. Bearer key.
use super::{Call, Found, Provider, Query, RESULTS, found, post, text};
use anyhow::{Context, Result};
use serde_json::json;

pub(crate) struct Perplexity;

#[async_trait::async_trait]
impl Provider for Perplexity {
    fn name(&self) -> &'static str {
        "perplexity"
    }
    fn base_url(&self) -> &'static str {
        "https://api.perplexity.ai"
    }
    fn reads_pages(&self) -> bool {
        false
    }
    async fn search(&self, call: &Call<'_>, query: &Query) -> Result<Vec<Found>> {
        let mut body =
            json!({"query": query.text, "max_results": RESULTS, "max_tokens_per_page": 512});
        if let Some(recent) = query.recent {
            body["search_recency_filter"] = json!(recent.word());
        }
        if !query.sites.is_empty() {
            body["search_domain_filter"] = json!(query.sites);
        }
        let auth = [("authorization", format!("Bearer {}", call.key))];
        let answer = post(&format!("{}/search", call.base), &auth, &body).await?;
        Ok(answer["results"]
            .as_array()
            .context("Perplexity's answer has no results")?
            .iter()
            .filter_map(|r| {
                let date = r["date"].as_str().map(str::to_owned);
                found(text(r, "title"), text(r, "url"), date, &text(r, "snippet"))
            })
            .collect())
    }
}
