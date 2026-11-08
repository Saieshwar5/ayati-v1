use super::*;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

type Seen = Arc<Mutex<Vec<(String, String, Value)>>>;
type Answer = Arc<dyn Fn(&str) -> (StatusCode, Value) + Send + Sync>;

/// A local stand-in for a provider's API: records path, key header and body.
async fn fake(
    answer: impl Fn(&str) -> (StatusCode, Value) + Send + Sync + 'static,
) -> (String, Seen) {
    let seen: Seen = Arc::default();
    let state: (Seen, Answer) = (seen.clone(), Arc::new(answer));
    let app = Router::new()
        .fallback(
            |State((seen, answer)): State<(Seen, Answer)>, request: Request| async move {
                let path = request.uri().path().to_owned();
                let headers = request.headers().clone();
                let key = ["authorization", "x-api-key"]
                    .iter()
                    .find_map(|h| headers.get(*h).and_then(|v| v.to_str().ok()))
                    .unwrap_or_default()
                    .to_owned();
                let bytes = axum::body::to_bytes(request.into_body(), 1 << 20)
                    .await
                    .unwrap();
                let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                seen.lock().unwrap().push((path.clone(), key, body));
                let (status, value) = answer(&path);
                (status, Json(value))
            },
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, seen)
}

fn file(json: Value) -> WebSearch {
    let dir = std::env::temp_dir().join(format!("web-search-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("key"), "secret-key\n").unwrap();
    let mut json = json;
    for entry in json["providers"].as_array_mut().unwrap() {
        entry["key_file"] = json!(dir.join("key"));
    }
    std::fs::write(dir.join("providers.json"), json.to_string()).unwrap();
    WebSearch::new(dir.join("providers.json"))
}

fn query() -> Query {
    Query {
        text: "teak price".into(),
        recent: Some(Recent::Month),
        sites: vec!["timber.example".into()],
    }
}

/// What each provider answers, in its own shape.
fn canned(provider: &str, path: &str) -> Value {
    match (provider, path) {
        ("tavily", "/search") => {
            json!({"results":[{"title":"Teak","url":"https://timber.example/teak","content":"Up 6%","published_date":"2026-11-01"}]})
        }
        ("tavily", "/extract") => {
            json!({"results":[{"url":"https://timber.example/teak","raw_content":"# Teak\nUp 6% since August."}]})
        }
        ("exa", "/search") => {
            json!({"results":[{"title":"Teak","url":"https://timber.example/teak","highlights":["Up 6%","Imports fell"],"publishedDate":"2026-11-01T00:00:00.000Z"}]})
        }
        ("exa", "/contents") => {
            json!({"results":[{"url":"https://timber.example/teak","title":"Teak","text":"Up 6% since August."}]})
        }
        ("perplexity", "/search") => {
            json!({"results":[{"title":"Teak","url":"https://timber.example/teak","snippet":"Up 6%","date":"2026-11-01"}]})
        }
        ("parallel", "/v1beta/search") => {
            json!({"results":[{"title":"Teak","url":"https://timber.example/teak","excerpts":["Up 6%"],"publish_date":"2026-11-01"}]})
        }
        ("parallel", "/v1beta/extract") => {
            json!({"results":[{"url":"https://timber.example/teak","title":"Teak","full_content":"Up 6% since August."}]})
        }
        _ => json!({"error":"unexpected"}),
    }
}

#[tokio::test]
async fn every_provider_searches_and_reads_in_cuckoos_terms() {
    for (name, key) in [
        ("tavily", "Bearer secret-key"),
        ("exa", "secret-key"),
        ("perplexity", "Bearer secret-key"),
        ("parallel", "secret-key"),
    ] {
        let provider = adapter(name).unwrap();
        let (base, seen) = fake(move |path| (StatusCode::OK, canned(name, path))).await;
        let read: Vec<&str> = if provider.reads_pages() {
            vec![name]
        } else {
            vec![]
        };
        let search = file(
            json!({"search":[name],"read":read,"providers":[{"provider":name,"key_file":"","base_url":base,"search_usd_per_1k":5,"read_usd_per_1k":1}]}),
        );
        let found = search.search(&query()).await.unwrap();
        assert_eq!(found.provider, name);
        assert!((found.cost_usd - 0.005).abs() < 1e-9, "{name}");
        assert_eq!(found.value.len(), 1, "{name}");
        assert_eq!(found.value[0].url, "https://timber.example/teak");
        assert!(found.value[0].passage.starts_with("Up 6%"), "{name}");
        assert_eq!(
            found.value[0].date.as_deref().map(|d| &d[..10]),
            Some("2026-11-01")
        );
        let (_, sent_key, body) = seen.lock().unwrap()[0].clone();
        assert_eq!(sent_key, key, "{name} sends its key its own way");
        assert!(body.to_string().contains("teak price"), "{name}");
        assert!(
            body.to_string().contains("timber.example"),
            "{name} limits sites"
        );
        let page = search.read("https://timber.example/teak").await;
        if provider.reads_pages() {
            let page = page.unwrap();
            assert!(page.value.text.contains("Up 6% since August"), "{name}");
            assert!((page.cost_usd - 0.001).abs() < 1e-9);
        } else {
            assert!(page.is_err(), "{name} only searches");
        }
    }
}

#[tokio::test]
async fn a_failing_provider_falls_back_to_the_next_in_order() {
    let (down, _) = fake(|_| (StatusCode::SERVICE_UNAVAILABLE, json!({}))).await;
    let (up, _) = fake(|path| (StatusCode::OK, canned("exa", path))).await;
    let search = file(
        json!({"search":["tavily","exa"],"read":["tavily","exa"],"providers":[
        {"provider":"tavily","key_file":"","base_url":down},
        {"provider":"exa","key_file":"","base_url":up}]}),
    );
    assert_eq!(search.search(&query()).await.unwrap().provider, "exa");
    assert_eq!(
        search
            .read("https://timber.example/teak")
            .await
            .unwrap()
            .provider,
        "exa"
    );
    let none = file(
        json!({"search":["tavily"],"providers":[{"provider":"tavily","key_file":"","base_url":down}]}),
    );
    let error = none.search(&query()).await.unwrap_err();
    assert!(format!("{error:#}").contains("No web search service answered"));
    assert!(
        none.read("https://timber.example/teak").await.is_err(),
        "no read order"
    );
}

#[test]
fn the_providers_file_is_checked() {
    let ok = |text: &str| ProvidersFile::parse(text).map(|_| ());
    let entry = |p: &str| format!(r#"{{"provider":"{p}","key_file":"/k"}}"#);
    ok(&format!(
        r#"{{"search":["tavily"],"read":["tavily"],"providers":[{}]}}"#,
        entry("tavily")
    ))
    .unwrap();
    for bad in [
        format!(r#"{{"search":[],"providers":[{}]}}"#, entry("tavily")),
        format!(r#"{{"search":["bing"],"providers":[{}]}}"#, entry("bing")),
        format!(r#"{{"search":["exa"],"providers":[{}]}}"#, entry("tavily")),
        format!(r#"{{"search":["perplexity"],"read":["perplexity"],"providers":[{}]}}"#, entry("perplexity")),
        format!(r#"{{"search":["tavily","tavily"],"providers":[{}]}}"#, entry("tavily")),
        format!(r#"{{"search":["tavily"],"providers":[{},{}]}}"#, entry("tavily"), entry("tavily")),
        r#"{"search":["tavily"],"providers":[{"provider":"tavily","key_file":"/k","base_url":"http://api.tavily.com"}]}"#.into(),
        r#"{"search":["tavily"],"providers":[{"provider":"tavily","key_file":"/k","search_usd_per_1k":-1}]}"#.into(),
        r#"{"search":["tavily"],"providers":[{"provider":"tavily","key_file":"/k","extra":1}]}"#.into(),
    ] {
        assert!(ok(&bad).is_err(), "{bad}");
    }
}

#[test]
fn only_public_web_addresses_are_read() {
    for good in [
        "https://timber.example/teak?x=1",
        "http://news.example.org/a",
    ] {
        public_url(good).unwrap();
    }
    for bad in [
        "file:///etc/passwd",
        "ftp://timber.example/",
        "http://localhost:8080/",
        "http://127.0.0.1/",
        "http://10.0.0.5/",
        "http://192.168.1.1/",
        "http://169.254.169.254/latest/meta-data",
        "http://100.64.0.1/",
        "http://[::1]/",
        "http://[fd00::1]/",
        "http://[::ffff:127.0.0.1]/",
        "http://intranet/",
        "http://db.internal/",
        "https://user:pass@timber.example/",
        "not a url",
    ] {
        assert!(public_url(bad).is_err(), "{bad}");
    }
}

#[test]
fn text_is_clipped_on_character_boundaries_and_dates_are_iso() {
    assert_eq!(clip("₹1,48,200", 2), "₹1");
    assert_eq!(clip("short", 10), "short");
    let date = exa::days_ago(0);
    assert_eq!(date.len(), 10);
    assert!(date.starts_with("20") && date.as_bytes()[4] == b'-');
}
