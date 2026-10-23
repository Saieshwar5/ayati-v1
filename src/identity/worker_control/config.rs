//! Local-only transport and operator-owned bootstrap file. AWS authentication is separate.
use anyhow::{Context, Result, ensure};
use std::{
    fs::OpenOptions,
    io::Read,
    net::SocketAddr,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

pub(crate) fn address(value: &str) -> Result<SocketAddr> {
    let address: SocketAddr = value
        .parse()
        .context("Use a loopback IP:port for worker control")?;
    ensure!(
        address.ip().is_loopback() && address.port() != 0,
        "Worker control is local-only and requires a nonzero port"
    );
    Ok(address)
}
pub(crate) fn url(value: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(value).context("Invalid worker control URL")?;
    let address = parsed.host_str().and_then(|host| {
        host.trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .ok()
    });
    ensure!(
        parsed.scheme() == "http"
            && address.is_some_and(|ip| ip.is_loopback())
            && parsed.port().is_some()
            && parsed.path() == "/"
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.query().is_none()
            && parsed.fragment().is_none(),
        "Worker control URL must be an explicit loopback HTTP origin"
    );
    Ok(parsed.origin().ascii_serialization())
}
pub(crate) fn key(path: &Path) -> Result<String> {
    ensure!(
        path.is_absolute(),
        "Worker control key file must be absolute"
    );
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .context("Cannot open worker control key file")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o777 == 0o600
            && metadata.len() <= 44,
        "Worker control key must be a private owner-owned regular file (0600)"
    );
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
            "https://127.0.0.1:8000",
            "http://127.0.0.1:8000/api",
            "http://user@127.0.0.1:8000",
            "http://127.0.0.1:8000?secret=x",
        ] {
            assert!(url(value).is_err());
        }
        assert!(url("http://127.0.0.1:8000").is_ok());
        assert!(address("0.0.0.0:8000").is_err());
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
