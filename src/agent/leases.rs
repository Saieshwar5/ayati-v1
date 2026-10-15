use anyhow::{Result, anyhow};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Serialize)]
pub(super) struct Activity {
    kind: &'static str,
    owner: String,
}

struct State {
    active: HashMap<String, Activity>,
    idle_since: Instant,
}

#[derive(Clone)]
pub(super) struct Leases(Arc<Mutex<State>>);

pub(super) struct Lease {
    leases: Leases,
    id: String,
}

impl Leases {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(State {
            active: HashMap::new(),
            idle_since: Instant::now(),
        })))
    }

    // Local, scoped ownership: no TTL may expire while a model/tool/approval
    // is still using the VM. Process recovery is handled by durable run claims.
    pub fn acquire(&self, kind: &'static str, owner: &str) -> Result<Lease> {
        let id = uuid::Uuid::new_v4().to_string();
        self.0
            .lock()
            .map_err(|_| anyhow!("Environment ownership unavailable"))?
            .active
            .insert(
                id.clone(),
                Activity {
                    kind,
                    owner: owner.into(),
                },
            );
        Ok(Lease {
            leases: self.clone(),
            id,
        })
    }

    pub fn idle_for(&self, duration: Duration) -> bool {
        self.0
            .lock()
            .is_ok_and(|state| state.active.is_empty() && state.idle_since.elapsed() >= duration)
    }

    pub fn active(&self) -> Result<Vec<Activity>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow!("Environment ownership unavailable"))?
            .active
            .values()
            .cloned()
            .collect())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.leases.0.lock() {
            state.active.remove(&self.id);
            if state.active.is_empty() {
                state.idle_since = Instant::now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_owners_must_release_before_idle_and_release_restarts_the_grace() {
        let leases = Leases::new();
        let run = leases.acquire("run", "job").unwrap();
        let interaction = leases.acquire("owner", "browser").unwrap();
        assert!(!leases.idle_for(Duration::ZERO));
        drop(run);
        assert!(!leases.idle_for(Duration::ZERO));
        drop(interaction);
        assert!(leases.idle_for(Duration::ZERO));
        assert!(!leases.idle_for(Duration::from_secs(60)));
    }
}
