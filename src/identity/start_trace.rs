//! Opt-in start timings: fixed code labels and durations, never context or credentials.
use std::{
    collections::BTreeMap,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
pub(crate) fn point(assignment: &str, side: &'static str, phase: &'static str) {
    if std::env::var("AYATI_TRACE_START").as_deref() != Ok("1") {
        return;
    }
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return;
    };
    let Ok(unix_us) = u64::try_from(now.as_micros()) else {
        return;
    };
    tracing::info!(target:"ayati::performance",assignment,side,start_point=%serde_json::json!({"version":1,"phase":phase,"unix_us":unix_us}),"Assignment start point");
}
pub(crate) struct StartTrace {
    enabled: bool,
    began: Instant,
    previous: Instant,
    phases: BTreeMap<&'static str, u64>,
}
impl StartTrace {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            enabled: std::env::var("AYATI_TRACE_START").as_deref() == Ok("1"),
            began: now,
            previous: now,
            phases: BTreeMap::new(),
        }
    }
    pub fn mark(&mut self, label: &'static str) {
        if self.enabled {
            self.phases.insert(
                label,
                u64::try_from(self.previous.elapsed().as_micros()).unwrap_or(u64::MAX),
            );
        }
        self.previous = Instant::now();
    }
    pub fn finish(&mut self, assignment: &str, side: &'static str) {
        if !self.enabled {
            return;
        }
        let total = u64::try_from(self.began.elapsed().as_micros()).unwrap_or(u64::MAX);
        tracing::info!(target:"ayati::performance",assignment,side,start_trace=%serde_json::json!({"version":1,"total_us":total,"phases_us":self.phases}),"Assignment start timing");
        self.enabled = false;
    }
}
