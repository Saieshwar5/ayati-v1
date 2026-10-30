//! TLS for worker control on a private network. The controller serves the
//! operator's certificate; executors trust only the operator's private
//! certificate authority, never public roots.
use anyhow::{Context, Result, ensure};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::{io::Read, net::SocketAddr, path::Path, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

const PEM_LIMIT: u64 = 64 * 1024;

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

fn read_public(path: &Path) -> Result<Vec<u8>> {
    ensure!(
        path.is_absolute(),
        "Worker control certificates must be absolute paths"
    );
    let metadata =
        std::fs::symlink_metadata(path).context("Cannot read worker control certificate")?;
    ensure!(
        metadata.is_file() && metadata.len() <= PEM_LIMIT,
        "Worker control certificates must be regular files of at most 64 KiB"
    );
    Ok(std::fs::read(path)?)
}

pub(crate) fn server_config(certificate: &Path, key: &Path) -> Result<Arc<rustls::ServerConfig>> {
    let chain = CertificateDer::pem_slice_iter(&read_public(certificate)?)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Invalid worker control certificate")?;
    ensure!(!chain.is_empty(), "Worker control certificate is empty");
    let mut pem = Vec::new();
    super::config::private(key, PEM_LIMIT)?.read_to_end(&mut pem)?;
    let key = PrivateKeyDer::from_pem_slice(&pem).context("Invalid worker control TLS key")?;
    let mut config = rustls::ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .context("Worker control certificate and key do not match")?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// Restrict a client to the private certificate authority in `ca`.
pub(crate) fn trust(builder: reqwest::ClientBuilder, ca: &Path) -> Result<reqwest::ClientBuilder> {
    let certificates = reqwest::Certificate::from_pem_bundle(&read_public(ca)?)
        .context("Invalid worker control certificate authority")?;
    ensure!(
        !certificates.is_empty(),
        "Worker control certificate authority is empty"
    );
    Ok(builder.https_only(true).tls_certs_only(certificates))
}

/// Accepts connections and completes TLS handshakes off the accept path, so a
/// slow or silent client cannot stall other executors.
pub(crate) struct Listener {
    local: SocketAddr,
    ready: mpsc::Receiver<(TlsStream<TcpStream>, SocketAddr)>,
}

impl Listener {
    pub fn new(listener: TcpListener, config: Arc<rustls::ServerConfig>) -> Result<Self> {
        let local = listener.local_addr()?;
        let (sender, ready) = mpsc::channel(64);
        let acceptor = TlsAcceptor::from(config);
        tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    _ = sender.closed() => break,
                    accepted = listener.accept() => accepted,
                };
                let Ok((stream, address)) = accepted else {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                };
                let (acceptor, sender) = (acceptor.clone(), sender.clone());
                tokio::spawn(async move {
                    let handshake =
                        tokio::time::timeout(Duration::from_secs(10), acceptor.accept(stream));
                    if let Ok(Ok(stream)) = handshake.await {
                        let _ = sender.send((stream, address)).await;
                    }
                });
            }
        });
        Ok(Self { local, ready })
    }
}

impl axum::serve::Listener for Listener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(connection) => connection,
            None => std::future::pending().await,
        }
    }
    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};

    /// Certificates from the operator tool, so the tool itself is exercised.
    fn certificates(dir: &Path) -> std::path::PathBuf {
        let out = dir.join(format!("certs-{}", uuid::Uuid::new_v4()));
        let status = std::process::Command::new(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/worker-control-certs.sh"),
        )
        .args([out.to_str().unwrap(), "localhost", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
        assert!(status.success());
        out
    }
    fn client(ca: &Path) -> reqwest::Client {
        trust(reqwest::Client::builder().no_proxy(), ca)
            .unwrap()
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn executors_reach_tls_worker_control_only_through_the_private_authority() {
        let dir = tempfile::tempdir().unwrap();
        let (good, other) = (certificates(dir.path()), certificates(dir.path()));
        let config = server_config(&good.join("server.pem"), &good.join("server.key")).unwrap();
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = tcp.local_addr().unwrap().port();
        let router = Router::new().route("/ping", get(|| async { "pong" }));
        let server =
            tokio::spawn(axum::serve(Listener::new(tcp, config).unwrap(), router).into_future());
        // A silent client that never sends a handshake must not block others.
        let _silent = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let url = format!("https://localhost:{port}/ping");
        let reply = client(&good.join("ca.pem")).get(&url).send().await.unwrap();
        assert_eq!(reply.text().await.unwrap(), "pong");
        assert!(
            client(&other.join("ca.pem"))
                .get(&url)
                .send()
                .await
                .is_err(),
            "another authority is refused"
        );
        let plain = reqwest::Client::builder().no_proxy().build().unwrap();
        assert!(
            plain
                .get(format!("http://localhost:{port}/ping"))
                .send()
                .await
                .is_err()
        );
        assert!(
            client(&good.join("ca.pem"))
                .get(format!("http://localhost:{port}/ping"))
                .send()
                .await
                .is_err(),
            "the executor client refuses plain HTTP"
        );
        server.abort();
    }

    #[test]
    fn the_server_key_must_be_private_and_match_its_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let (good, other) = (certificates(dir.path()), certificates(dir.path()));
        use std::os::unix::fs::PermissionsExt;
        let key = good.join("server.key");
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(server_config(&good.join("server.pem"), &key).is_err());
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(server_config(&good.join("server.pem"), &other.join("server.key")).is_err());
        assert!(server_config(&good.join("server.pem"), &key).is_ok());
        assert!(server_config(Path::new("relative.pem"), &key).is_err());
    }
}
