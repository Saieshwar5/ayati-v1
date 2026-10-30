//! Worker control transport rules and operator-owned secret files. AWS identity
//! proof is separate.
use anyhow::{Context, Result, ensure};
use std::{
    fs::OpenOptions,
    io::Read,
    net::{IpAddr, SocketAddr},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

/// Where executors reach worker control: plain HTTP only on a loopback IP for
/// local development, otherwise HTTPS to a named host on a private network.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Endpoint {
    pub origin: String,
    /// `host:port`, checked against every request's Host header.
    pub host: String,
    pub tls: bool,
}

pub(crate) fn url(value: &str) -> Result<Endpoint> {
    let parsed = reqwest::Url::parse(value).context("Invalid worker control URL")?;
    let host = parsed
        .host_str()
        .context("Worker control URL needs a host")?;
    let loopback = host
        .trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback());
    ensure!(
        (parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback))
            && parsed.port().is_some_and(|port| port != 0)
            && parsed.path() == "/"
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.query().is_none()
            && parsed.fragment().is_none(),
        "Worker control URL must be https://host:port, or http:// with a loopback IP"
    );
    let origin = parsed.origin().ascii_serialization();
    Ok(Endpoint {
        host: origin
            .split_once("://")
            .map(|(_, host)| host.to_owned())
            .unwrap_or_default(),
        tls: parsed.scheme() == "https",
        origin,
    })
}

/// Only TLS may listen beyond loopback.
pub(crate) fn listen(address: SocketAddr, tls: bool) -> Result<SocketAddr> {
    ensure!(
        address.port() != 0 && (tls || address.ip().is_loopback()),
        "Worker control needs TLS to listen beyond loopback"
    );
    Ok(address)
}

/// A secret file the operator provisioned for this process alone.
pub(crate) fn private(path: &Path, max: u64) -> Result<std::fs::File> {
    ensure!(
        path.is_absolute(),
        "Worker control secret files must be absolute"
    );
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .context("Cannot open worker control secret file")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o777 == 0o600
            && metadata.len() <= max,
        "Worker control secrets must be private owner-owned regular files (0600)"
    );
    Ok(file)
}

pub(crate) fn key(path: &Path) -> Result<String> {
    let file = private(path, 44)?;
    let mut secret = String::new();
    file.take(45).read_to_string(&mut secret)?;
    let secret = secret.trim_end_matches('\n');
    super::model::token(secret)?;
    Ok(secret.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn machine_transport_and_key_are_local_and_private() {
        for value in [
            "http://example.com:8000",
            "http://10.0.0.5:8000",
            "https://worker.internal",
            "http://127.0.0.1:8000/api",
            "https://user@worker.internal:8443",
            "http://127.0.0.1:8000?secret=x",
            "ftp://127.0.0.1:8000",
        ] {
            assert!(url(value).is_err(), "{value}");
        }
        assert!(!url("http://127.0.0.1:8000").unwrap().tls);
        let private = url("https://worker.internal:8443").unwrap();
        assert_eq!(
            (private.host.as_str(), private.tls),
            ("worker.internal:8443", true)
        );
        assert!(listen("0.0.0.0:8000".parse().unwrap(), false).is_err());
        assert!(listen("0.0.0.0:8000".parse().unwrap(), true).is_ok());
        assert!(listen("127.0.0.1:0".parse().unwrap(), false).is_err());
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bootstrap");
        std::fs::write(&path, crate::identity::provider::random_token()).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(key(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(key(&path).is_ok());
        std::os::unix::fs::symlink(&path, temp.path().join("link")).unwrap();
        assert!(key(&temp.path().join("link")).is_err());
    }
}
