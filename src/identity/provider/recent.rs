//! How long Cuckoo trusts WorkOS's last answer that a session is live.
//!
//! Every request, and every batch of each open live connection, checks the
//! session. Asking WorkOS each time costs two API calls, so about 100 idle tabs
//! (live connections and visible pages poll) would reach WorkOS's user-management
//! limit and every request would then fail.
//!
//! Reads (GET, HEAD and live stream batches) trust a positive answer for
//! [`TRUST`]. Anything that changes state asks WorkOS live, so someone blocked
//! in WorkOS can't send, edit or approve anything; at most they may keep reading
//! what they could read a minute ago. Cuckoo's own checks are never cached: the
//! session row (logout deletes it), its expiry, workspace membership and
//! conversation access are read from PostgreSQL on every request. Negative
//! answers and errors are never trusted.
use super::Provider;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(crate) const TRUST: Duration = Duration::from_secs(60);
/// Bounds memory: past this many trusted sessions, expired ones are dropped.
const MAX_ENTRIES: usize = 50_000;

/// WorkOS user, WorkOS session and the email Cuckoo expects.
type Key = (String, String, String);

/// One per provider client (one per server process in production), so separate
/// test servers never share answers.
#[derive(Default)]
pub(crate) struct Trusted(Mutex<HashMap<Key, Instant>>);

impl Provider {
    /// [`Provider::identity_valid`]; for reads, answered from a positive result
    /// at most [`TRUST`] old. Live answers refresh what reads may trust.
    pub async fn identity_valid_checked(
        &self,
        user: &str,
        session: &str,
        email: &str,
        reading: bool,
    ) -> Result<bool> {
        self.identity_valid_at(user, session, email, reading, Instant::now())
            .await
    }

    pub(crate) async fn identity_valid_at(
        &self,
        user: &str,
        session: &str,
        email: &str,
        reading: bool,
        now: Instant,
    ) -> Result<bool> {
        let key = (user.to_owned(), session.to_owned(), email.to_owned());
        self.remember(key, reading, now, self.identity_valid(user, session, email))
            .await
    }

    /// [`Provider::account_valid`] for device sessions, trusted for reads the
    /// same way. Their key has an empty session; WorkOS session IDs never are.
    pub async fn account_valid_checked(
        &self,
        user: &str,
        email: &str,
        reading: bool,
    ) -> Result<bool> {
        let key = (user.to_owned(), String::new(), email.to_owned());
        self.remember(
            key,
            reading,
            Instant::now(),
            self.account_valid(user, email),
        )
        .await
    }

    /// Answers a read from a fresh positive result, or asks WorkOS (`check`)
    /// and keeps a positive answer.
    async fn remember(
        &self,
        key: Key,
        reading: bool,
        now: Instant,
        check: impl std::future::Future<Output = Result<bool>>,
    ) -> Result<bool> {
        if reading && self.trusted.fresh(&key, now) {
            return Ok(true);
        }
        let valid = check.await?;
        let mut map = self.trusted.0.lock().unwrap();
        if valid {
            if map.len() >= MAX_ENTRIES {
                map.retain(|_, checked| now.saturating_duration_since(*checked) < TRUST);
            }
            if map.len() < MAX_ENTRIES {
                map.insert(key, now);
            }
        } else {
            map.remove(&key);
        }
        Ok(valid)
    }
}

impl Trusted {
    fn fresh(&self, key: &Key, now: Instant) -> bool {
        self.0
            .lock()
            .unwrap()
            .get(key)
            .is_some_and(|checked| now.saturating_duration_since(*checked) < TRUST)
    }
}
