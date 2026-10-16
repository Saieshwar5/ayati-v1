//! Explicit browser egress; restricted QEMU networking has no direct Internet route.
use anyhow::{Context, Result, ensure};
use std::{net::IpAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

pub struct BrowserNetwork {
    pub port: u16,
    task: JoinHandle<()>,
    shutdown: tokio_util::sync::CancellationToken,
}

impl BrowserNetwork {
    pub async fn start() -> Result<Self> {
        // Only explicit test processes can map this reserved synthetic origin.
        // Normal public browsing still rejects every loopback/private target.
        let fixture = std::env::var("AYATI_BROWSER_FIXTURE_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .filter(|p| *p != 0);
        Self::start_with_fixture(fixture).await
    }

    async fn start_with_fixture(fixture: Option<u16>) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let shutdown = tokio_util::sync::CancellationToken::new();
        let cancel = shutdown.clone();
        let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(32));
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let Ok(permit) = permits.clone().try_acquire_owned() else {
                    continue;
                };
                let stopped = cancel.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    tokio::select! {
                        _ = stopped.cancelled() => {},
                        _ = tokio::time::timeout(Duration::from_secs(300), proxy(stream, fixture)) => {},
                    }
                });
            }
        });
        Ok(Self {
            port,
            task,
            shutdown,
        })
    }
}

impl Drop for BrowserNetwork {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.task.abort();
    }
}

fn public_ip(ip: IpAddr) -> bool {
    let IpAddr::V4(ip) = ip else {
        return false;
    };
    let [a, b, c, _] = ip.octets();
    !matches!(a, 0 | 10 | 127 | 224..=255)
        && !(a == 100 && (64..=127).contains(&b))
        && !(a == 169 && b == 254)
        && !(a == 172 && (16..=31).contains(&b))
        && !(a == 192 && (b == 168 || b == 0 || (b == 88 && c == 99)))
        && !(a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
        && !(a == 203 && b == 0 && c == 113)
}

async fn proxy(mut client: TcpStream, fixture: Option<u16>) -> Result<()> {
    let result = connect(&mut client, fixture).await;
    match result {
        Ok(mut remote) => {
            tokio::io::copy_bidirectional(&mut client, &mut remote).await?;
        }
        Err(_) => {
            let _ = client
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                )
                .await;
        }
    }
    Ok(())
}

async fn connect(client: &mut TcpStream, fixture: Option<u16>) -> Result<TcpStream> {
    let header = tokio::time::timeout(Duration::from_secs(10), async {
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            ensure!(bytes.len() < 16384, "Proxy header too large");
            bytes.push(client.read_u8().await?);
        }
        Ok::<_, anyhow::Error>(String::from_utf8(bytes)?)
    })
    .await??;
    let first = header.lines().next().context("Missing proxy request")?;
    let parts: Vec<_> = first.split_whitespace().collect();
    ensure!(
        parts.len() == 3 && ["HTTP/1.0", "HTTP/1.1"].contains(&parts[2]),
        "Invalid proxy request"
    );
    let tunnel = parts[0] == "CONNECT";
    let url = reqwest::Url::parse(&if tunnel {
        format!("https://{}", parts[1])
    } else {
        parts[1].into()
    })?;
    ensure!(
        url.username().is_empty() && url.password().is_none() && url.fragment().is_none(),
        "Invalid proxy destination"
    );
    ensure!(
        if tunnel {
            url.path() == "/" && url.query().is_none()
        } else {
            url.scheme() == "http"
        },
        "Unsupported proxy protocol"
    );
    let host = url.host_str().context("Missing proxy destination")?;
    let port = url
        .port_or_known_default()
        .context("Missing destination port")?;
    ensure!([80, 443].contains(&port), "Unsupported destination port");
    let fixture_address = fixture.filter(|_| host == "fixture.ayati.invalid" && port == 80);
    let addresses: Vec<_> = if let Some(port) = fixture_address {
        vec![std::net::SocketAddr::from(([127, 0, 0, 1], port))]
    } else {
        tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::lookup_host((host, port)),
        )
        .await??
        .filter(|a| matches!(a.ip(), IpAddr::V4(_)))
        .collect()
    };
    ensure!(
        !addresses.is_empty()
            && (fixture_address.is_some() || addresses.iter().all(|a| public_ip(a.ip()))),
        "Private/internal destination denied"
    );
    let mut remote =
        tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(addresses[0])).await??;
    if tunnel {
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
    } else {
        // Preserve the target's path; remove proxy-specific and Host headers and
        // supply the validated authority. A private Host header cannot reroute it.
        let mut path = url.path().to_owned();
        if let Some(query) = url.query() {
            path.push('?');
            path.push_str(query);
        }
        let mut forwarded = format!(
            "{} {} {}\r\nHost: {}:{}\r\nConnection: close\r\n",
            parts[0], path, parts[2], host, port
        );
        for line in header.lines().skip(1).filter(|line| !line.is_empty()) {
            let (name, _) = line.split_once(':').context("Invalid proxy header")?;
            ensure!(!line.contains('\0'), "Invalid proxy header");
            if ![
                "host",
                "proxy-authorization",
                "proxy-connection",
                "connection",
            ]
            .contains(&name.to_ascii_lowercase().as_str())
            {
                forwarded.push_str(line);
                forwarded.push_str("\r\n");
            }
        }
        forwarded.push_str("\r\n");
        remote.write_all(forwarded.as_bytes()).await?;
    }
    Ok(remote)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn denies_private_and_special_ranges() {
        for ip in [
            "0.0.0.0",
            "10.0.2.2",
            "100.100.100.200",
            "127.0.0.1",
            "169.254.169.254",
            "172.17.0.1",
            "192.168.1.1",
            "198.18.0.1",
            "224.0.0.1",
            "::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip(std::net::Ipv4Addr::new(93, 184, 215, 14).into()));
    }
    #[tokio::test]
    async fn real_proxy_rejects_internal_targets_and_unapproved_ports() {
        let proxy = BrowserNetwork::start().await.unwrap();
        for request in [
            "CONNECT 127.0.0.1:80 HTTP/1.1\r\n\r\n",
            "GET http://169.254.169.254/latest HTTP/1.1\r\n\r\n",
            "CONNECT example.com:22 HTTP/1.1\r\n\r\n",
        ] {
            let mut socket = TcpStream::connect(("127.0.0.1", proxy.port)).await.unwrap();
            socket.write_all(request.as_bytes()).await.unwrap();
            let mut response = String::new();
            socket.read_to_string(&mut response).await.unwrap();
            assert!(response.starts_with("HTTP/1.1 403"));
        }
    }

    #[tokio::test]
    async fn releasing_the_vm_network_closes_existing_browser_tunnels() {
        let remote = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = BrowserNetwork::start_with_fixture(Some(remote.local_addr().unwrap().port()))
            .await
            .unwrap();
        let mut client = TcpStream::connect(("127.0.0.1", proxy.port)).await.unwrap();
        client
            .write_all(b"CONNECT fixture.ayati.invalid:80 HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        let (mut upstream, _) = remote.accept().await.unwrap();
        let expected = b"HTTP/1.1 200 Connection Established\r\n\r\n";
        let mut header = vec![0; expected.len()];
        client.read_exact(&mut header).await.unwrap();
        assert_eq!(header, expected);
        drop(proxy);
        let mut tail = Vec::new();
        tokio::time::timeout(Duration::from_secs(1), client.read_to_end(&mut tail))
            .await
            .unwrap()
            .unwrap();
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), upstream.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }
}
