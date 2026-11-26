//! Starting sign-in needs no account and saves a row (`postgres/login_flows.rs`),
//! so each client address may start only a few in a window. Counted in this
//! controller process: every replica allows this many, and the database's
//! storage guard holds behind them. Without it, anyone could fill the shared
//! sign-in table and lock every person out of signing in.
use axum::http::HeaderMap;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv6Addr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};

/// Sign-ins one address may start in [`WINDOW`]; a person needs one or two.
pub(crate) const PER_ADDRESS: u32 = 20;
/// As long as a started sign-in lasts.
const WINDOW: Duration = Duration::from_secs(600);
/// Addresses remembered at once. Past it, new addresses are let through and the
/// database's storage guard limits them; that many is an attack for a firewall.
const ADDRESSES: usize = 100_000;

#[derive(Default)]
pub(super) struct SignInStarts {
    seen: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

impl SignInStarts {
    /// Whether `client` may start another sign-in now; counts it if so.
    pub(super) fn allow(&self, client: IpAddr) -> bool {
        self.allow_at(client, Instant::now())
    }

    fn allow_at(&self, client: IpAddr, now: Instant) -> bool {
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        if seen.len() >= ADDRESSES {
            seen.retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
        }
        if seen.len() >= ADDRESSES && !seen.contains_key(&client) {
            return true;
        }
        let entry = seen.entry(client).or_insert((now, 0));
        if now.duration_since(entry.0) >= WINDOW {
            *entry = (now, 0);
        }
        if entry.1 >= PER_ADDRESS {
            return false;
        }
        entry.1 += 1;
        true
    }
}

/// The address a request came from. Behind the hosted TLS proxy (a load
/// balancer appends the address it saw) that is the last `X-Forwarded-For`
/// entry; otherwise the connection's own. IPv6 addresses count per /64, since
/// one network is usually given a whole /64.
pub(super) fn client(headers: &HeaderMap, peer: Option<SocketAddr>, proxied: bool) -> IpAddr {
    let forwarded = proxied
        .then(|| {
            headers
                .get_all("x-forwarded-for")
                .iter()
                .next_back()?
                .to_str()
                .ok()?
                .rsplit(',')
                .next()?
                .trim()
                .parse::<IpAddr>()
                .ok()
        })
        .flatten();
    let address = forwarded
        .or(peer.map(|p| p.ip()))
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));
    match address {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = v6.segments();
                IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        },
        v4 => v4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_address_may_start_a_few_sign_ins_in_a_window() {
        let starts = SignInStarts::default();
        let now = Instant::now();
        let one: IpAddr = "203.0.113.7".parse().unwrap();
        let other: IpAddr = "198.51.100.9".parse().unwrap();
        for _ in 0..PER_ADDRESS {
            assert!(starts.allow_at(one, now));
        }
        assert!(!starts.allow_at(one, now), "the next one is refused");
        assert!(starts.allow_at(other, now), "other people still sign in");
        assert!(
            starts.allow_at(one, now + WINDOW),
            "and it starts over later"
        );
    }

    #[test]
    fn the_address_is_the_proxys_last_entry_only_behind_the_proxy() {
        let peer: SocketAddr = "10.0.0.5:4000".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 203.0.113.7".parse().unwrap());
        let seen = |h: &HeaderMap, proxied| client(h, Some(peer), proxied).to_string();
        // The client may write the first entries; the proxy appends the last.
        assert_eq!(seen(&headers, true), "203.0.113.7");
        assert_eq!(
            seen(&headers, false),
            "10.0.0.5",
            "not hosted: the connection"
        );
        assert_eq!(seen(&HeaderMap::new(), true), "10.0.0.5");
        headers.insert("x-forwarded-for", "2001:db8:1:2:aaaa::1".parse().unwrap());
        assert_eq!(seen(&headers, true), "2001:db8:1:2::", "one /64");
        assert_eq!(client(&HeaderMap::new(), None, true).to_string(), "0.0.0.0");
    }
}
