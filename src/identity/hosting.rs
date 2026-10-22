//! Explicit public origin behind a TLS proxy; forwarded headers never establish trust.
use super::*;
use std::net::{IpAddr, Ipv4Addr};

pub(super) struct Hosting {
    pub origin: String,
    pub bind: IpAddr,
}
impl Hosting {
    pub fn from_env(port: u16) -> Result<Self> {
        let providers = std::env::var("AYATI_WORKER_PROVIDERS").unwrap_or_default();
        anyhow::ensure!(
            providers.is_empty()
                || providers
                    .split(',')
                    .all(|p| matches!(p, "openai" | "fireworks-ai")),
            "AYATI_WORKER_PROVIDERS must be a comma-separated list of openai,fireworks-ai"
        );
        let hosting = Self::parse(
            std::env::var("AYATI_PUBLIC_ORIGIN").ok().as_deref(),
            std::env::var("AYATI_BIND_ADDRESS").ok().as_deref(),
            port,
        )?;
        if hosting.origin.starts_with("https://") {
            anyhow::ensure!(
                std::env::var("RELAY_DATABASE_URL").is_ok()
                    && std::env::var("AYATI_CHAT_EXECUTION").as_deref() == Ok("external"),
                "Hosted identity requires PostgreSQL and AYATI_CHAT_EXECUTION=external"
            );
            anyhow::ensure!(
                std::env::var_os("CUCKOO_LIBRARY_LOCAL_ROOT").is_none(),
                "Hosted files require private S3 storage, not a local Library directory"
            );
        }
        Ok(hosting)
    }
    fn parse(origin: Option<&str>, bind: Option<&str>, port: u16) -> Result<Self> {
        let bind = bind
            .map(str::parse::<IpAddr>)
            .transpose()
            .context("AYATI_BIND_ADDRESS must be an IP address")?
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let origin = if let Some(origin) = origin {
            let url = reqwest::Url::parse(origin).context("Invalid AYATI_PUBLIC_ORIGIN")?;
            anyhow::ensure!(
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.path() == "/"
                    && url.query().is_none()
                    && url.fragment().is_none(),
                "AYATI_PUBLIC_ORIGIN must be an HTTPS origin without credentials, path or query"
            );
            url.origin().ascii_serialization()
        } else {
            anyhow::ensure!(
                bind == IpAddr::V4(Ipv4Addr::LOCALHOST),
                "A non-default bind address requires an HTTPS AYATI_PUBLIC_ORIGIN"
            );
            format!("http://127.0.0.1:{port}")
        };
        Ok(Self { origin, bind })
    }
}
pub(super) fn hosted(app: &App) -> bool {
    app.origin.starts_with("https://")
}
pub(super) fn provider_configured(provider: &str, key: &str) -> bool {
    if std::env::var("AYATI_CHAT_EXECUTION").as_deref() == Ok("external") {
        // Operator-declared capability, not a live worker availability probe.
        std::env::var("AYATI_WORKER_PROVIDERS")
            .unwrap_or_default()
            .split(',')
            .any(|p| p == provider)
    } else {
        std::env::var(key).is_ok_and(|s| !s.trim().is_empty())
    }
}
pub(super) async fn live() -> StatusCode {
    StatusCode::OK
}
pub(super) async fn ready(State(app): State<Shared>) -> StatusCode {
    let probe = async {
        match &app.store {
            database::Database::Postgres(pg) => pg.ping().await,
            database::Database::Sqlite(store) => {
                store
                    .0
                    .lock()
                    .unwrap()
                    .query_row("SELECT 1", [], |_| Ok(()))?;
                Ok(())
            }
        }
    };
    if matches!(
        tokio::time::timeout(std::time::Duration::from_secs(3), probe).await,
        Ok(Ok(()))
    ) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_binding_requires_explicit_https_origin() {
        let local = Hosting::parse(None, None, 8765).unwrap();
        assert_eq!(local.origin, "http://127.0.0.1:8765");
        assert!(local.bind.is_loopback());
        assert!(Hosting::parse(None, Some("0.0.0.0"), 8765).is_err());
        for invalid in [
            "http://example.test",
            "https://user:secret@example.test",
            "https://example.test/path",
            "https://example.test/?token=secret",
            "https://example.test/#fragment",
            "not a URL",
        ] {
            assert!(Hosting::parse(Some(invalid), None, 8765).is_err());
        }
        let public =
            Hosting::parse(Some("https://Example.test:443/"), Some("0.0.0.0"), 8765).unwrap();
        assert_eq!(public.origin, "https://example.test");
        assert!(public.bind.is_unspecified());
        assert!(Hosting::parse(Some("https://example.test"), Some("hostname"), 8765).is_err());
    }
}
