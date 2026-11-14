use super::*;
use axum::{http::header, response::IntoResponse, routing::get as route};

#[test]
fn the_first_web_link_is_found_without_trailing_punctuation() {
    assert_eq!(
        first_link("Prices: https://shree.example/prices?x=1, see you").as_deref(),
        Some("https://shree.example/prices?x=1")
    );
    assert_eq!(
        first_link("[the list](https://a.example/list). Also http://b.example").as_deref(),
        Some("https://a.example/list")
    );
    assert_eq!(first_link("No links here, just https:// alone"), None);
    assert_eq!(first_link("ftp://files.example/x"), None);
}

#[test]
fn pages_describe_themselves_with_open_graph_then_plain_tags() {
    let url = reqwest::Url::parse("https://www.shree.example/prices").unwrap();
    let html = r#"<html><head><TITLE>Fallback</TITLE>
        <meta property="og:title" content="Shree Plywood &amp; Boards — Prices 2027">
        <meta name='description' content='Plain description'>
        <meta content="Rates for &quot;BWR&quot; ply" property=og:description />
        </head><body><meta property="og:title" content="Not in the head"></body></html>"#;
    let preview = read_meta(html, &url).unwrap();
    assert_eq!(preview.title, "Shree Plywood & Boards — Prices 2027");
    assert_eq!(
        preview.site, "shree.example",
        "the host without www when no site name"
    );
    assert_eq!(
        preview.description.as_deref(),
        Some("Rates for \"BWR\" ply")
    );
    let plain = read_meta(
        "<head><title>\n  Only a   title &#8212; ok\n</title></head>",
        &url,
    )
    .unwrap();
    assert_eq!(
        (plain.title.as_str(), plain.description),
        ("Only a title — ok", None)
    );
    assert_eq!(
        read_meta("<head><meta name=description content=x></head>", &url),
        None,
        "no title, no preview"
    );
    let long = format!("<head><title>{}</title></head>", "é".repeat(400));
    assert_eq!(read_meta(&long, &url).unwrap().title.chars().count(), 200);
    // A stray ampersand near multibyte text is kept, never split.
    assert_eq!(clean("A & ब ; x &zz; y"), "A & ब ; x &zz; y");
}

#[tokio::test]
async fn previews_follow_redirects_read_only_html_and_refuse_private_addresses() {
    let app = axum::Router::new()
        .route("/page", route(|| async {
            ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], "<head><title>Dispatch board</title><meta property=og:site_name content=Board></head>").into_response()
        }))
        .route("/moved", route(|| async { (axum::http::StatusCode::FOUND, [(header::LOCATION, "/page")]).into_response() }))
        .route("/file", route(|| async { ([(header::CONTENT_TYPE, "application/pdf")], "%PDF").into_response() }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let at = |path: &str| reqwest::Url::parse(&format!("http://{address}{path}")).unwrap();

    let preview = fetch(at("/moved"), false).await.unwrap().unwrap();
    assert_eq!(
        (preview.title.as_str(), preview.site.as_str()),
        ("Dispatch board", "Board")
    );
    assert!(preview.url.ends_with("/page"), "the address it ended at");
    assert_eq!(
        fetch(at("/file"), false).await.unwrap(),
        None,
        "not a web page"
    );
    assert_eq!(fetch(at("/missing"), false).await.unwrap(), None);

    // Fenced: this machine, private networks and odd ports are never fetched.
    for refused in [
        format!("http://{address}/page"),
        "http://127.0.0.1/page".into(),
        "http://localhost/page".into(),
        "http://10.0.0.8/".into(),
        "http://169.254.169.254/latest/meta-data/".into(),
        "https://user:pass@example.com/".into(),
        "file:///etc/passwd".into(),
    ] {
        assert!(
            fetch(reqwest::Url::parse(&refused).unwrap(), true)
                .await
                .is_err(),
            "{refused}"
        );
    }
}
