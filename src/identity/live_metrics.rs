//! How live messaging is doing on this server, for Cuckoo Control's Overview.
//!
//! - **Delivery**: from a change being saved (`relay_sync_events.created_ms`) to
//!   this server pushing it to a recipient's live connection. Catch-up pages after
//!   a reconnect are left out, so this measures live delivery only.
//! - **Sending**: how long the send request took on the server, sign-in check,
//!   access checks and the database write included.
//! - **Listener**: whether this server hears PostgreSQL's change notifications.
//!
//! Figures are per server process and cover the last [`WINDOW`]. They are counts
//! and timings only, never message contents.
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(600);
/// Bounds memory on a busy server; the newest samples are kept.
const MAX_SAMPLES: usize = 20_000;

struct Samples(Mutex<VecDeque<(Instant, u32)>>);
impl Samples {
    const fn new() -> Self {
        Self(Mutex::new(VecDeque::new()))
    }
    fn add(&self, ms: u64) {
        let mut samples = self.0.lock().unwrap();
        if samples.len() >= MAX_SAMPLES {
            samples.pop_front();
        }
        samples.push_back((Instant::now(), ms.min(u32::MAX as u64) as u32));
    }
    /// Count, median, 95th percentile and slowest in the window.
    fn summary(&self) -> Value {
        let mut samples = self.0.lock().unwrap();
        while samples.front().is_some_and(|(at, _)| at.elapsed() > WINDOW) {
            samples.pop_front();
        }
        let mut values: Vec<u32> = samples.iter().map(|(_, ms)| *ms).collect();
        values.sort_unstable();
        let at = |q: f64| {
            values
                .get(((values.len() as f64 - 1.0) * q).round() as usize)
                .copied()
        };
        json!({"count": values.len(), "p50_ms": at(0.5), "p95_ms": at(0.95), "max_ms": values.last()})
    }
}

static DELIVERY: Samples = Samples::new();
static SENDING: Samples = Samples::new();
static STREAMS_OPENED: AtomicU64 = AtomicU64::new(0);
static LISTENING: AtomicBool = AtomicBool::new(false);
static LISTENER_CONNECTS: AtomicU64 = AtomicU64::new(0);

/// A change saved at `saved_ms` (Unix milliseconds) was just pushed to a viewer.
pub(crate) fn delivered(saved_ms: i64) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    DELIVERY.add(now_ms.saturating_sub(saved_ms).max(0) as u64);
}

pub(crate) fn sent(took: Duration) {
    SENDING.add(took.as_millis() as u64);
}

pub(crate) fn stream_opened() {
    STREAMS_OPENED.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn listener(connected: bool) {
    if connected {
        LISTENER_CONNECTS.fetch_add(1, Ordering::Relaxed);
    }
    LISTENING.store(connected, Ordering::Relaxed);
}

pub(crate) fn snapshot() -> Value {
    json!({
        "open_streams": super::chat::open_streams(),
        "streams_opened": STREAMS_OPENED.load(Ordering::Relaxed),
        "listening": LISTENING.load(Ordering::Relaxed),
        "listener_connects": LISTENER_CONNECTS.load(Ordering::Relaxed),
        "delivery": DELIVERY.summary(),
        "sending": SENDING.summary(),
        "window_seconds": WINDOW.as_secs(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_give_count_median_tail_and_slowest() {
        let samples = Samples::new();
        assert_eq!(
            samples.summary(),
            json!({"count": 0, "p50_ms": null, "p95_ms": null, "max_ms": null})
        );
        for ms in 1..=100 {
            samples.add(ms);
        }
        assert_eq!(
            samples.summary(),
            json!({"count": 100, "p50_ms": 51, "p95_ms": 95, "max_ms": 100})
        );
    }
}
