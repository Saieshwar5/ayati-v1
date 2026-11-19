//! Where browsers reach the controller and the address it binds.
use crate::config::Reader;
use std::net::{IpAddr, Ipv4Addr};

/// HTTPS public origin (behind a TLS proxy) or loopback-only local HTTP.
pub(super) fn origin(env: &mut Reader, port: u16) -> (String, IpAddr) {
    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let bind = match env.text("AYATI_BIND_ADDRESS").map(|v| v.parse::<IpAddr>()) {
        None => local,
        Some(Ok(bind)) => bind,
        Some(Err(_)) => {
            env.problem("AYATI_BIND_ADDRESS must be an IP address");
            local
        }
    };
    let Some(origin) = env.text("AYATI_PUBLIC_ORIGIN") else {
        if bind != local {
            env.problem("A non-default bind address requires an HTTPS AYATI_PUBLIC_ORIGIN");
        }
        return (format!("http://127.0.0.1:{port}"), bind);
    };
    match reqwest::Url::parse(&origin) {
        Ok(url)
            if url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none() =>
        {
            (url.origin().ascii_serialization(), bind)
        }
        _ => {
            env.problem(
                "AYATI_PUBLIC_ORIGIN must be an HTTPS origin without credentials, path or query",
            );
            (format!("http://127.0.0.1:{port}"), bind)
        }
    }
}
