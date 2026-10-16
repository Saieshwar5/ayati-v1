use anyhow::{Result, ensure};

// Shared by VM execution and the explicit Settings connection test.
pub fn endpoint(provider: &str) -> Result<(reqwest::Url, &'static str)> {
    let (url, key_name) = match provider {
        "fireworks-ai" => (
            "https://api.fireworks.ai/inference/v1/chat/completions".into(),
            "FIREWORKS_API_KEY",
        ),
        "openai" => {
            let host =
                std::env::var("OPENAI_HOST").unwrap_or_else(|_| "https://api.openai.com".into());
            let base =
                std::env::var("OPENAI_BASE_PATH").unwrap_or_else(|_| "v1/chat/completions".into());
            (
                format!("{}/{}", host.trim_end_matches('/'), base.trim_matches('/')),
                "OPENAI_API_KEY",
            )
        }
        _ => anyhow::bail!("Use the supported fireworks-ai or openai provider"),
    };
    let url = reqwest::Url::parse(&url)?;
    ensure!(
        url.scheme() == "https"
            || (url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))),
        "Model endpoint must use HTTPS; HTTP fixtures must be loopback"
    );
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Model endpoint cannot contain credentials or query parameters"
    );
    Ok((url, key_name))
}
