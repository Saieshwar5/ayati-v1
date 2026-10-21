//! Per-process admission prevents one account/workspace exhausting live connections.
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
#[derive(Default)]
struct Counts {
    total: usize,
    users: HashMap<(String, String), usize>,
    workspaces: HashMap<String, usize>,
}
static COUNTS: OnceLock<Mutex<Counts>> = OnceLock::new();
pub(super) struct Permit {
    workspace: String,
    user: String,
}
impl Permit {
    pub fn acquire(workspace: &str, user: &str) -> Option<Self> {
        let mut counts = COUNTS.get_or_init(Default::default).lock().ok()?;
        let key = (workspace.to_owned(), user.to_owned());
        if counts.total >= 512
            || counts.users.get(&key).copied().unwrap_or(0) >= 6
            || counts.workspaces.get(workspace).copied().unwrap_or(0) >= 128
        {
            return None;
        }
        counts.total += 1;
        *counts.users.entry(key).or_default() += 1;
        *counts.workspaces.entry(workspace.into()).or_default() += 1;
        Some(Self {
            workspace: workspace.into(),
            user: user.into(),
        })
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        if let Ok(mut counts) = COUNTS.get_or_init(Default::default).lock() {
            counts.total = counts.total.saturating_sub(1);
            let key = (self.workspace.clone(), self.user.clone());
            if let Some(n) = counts.users.get_mut(&key) {
                *n -= 1;
                if *n == 0 {
                    counts.users.remove(&key);
                }
            }
            if let Some(n) = counts.workspaces.get_mut(&self.workspace) {
                *n -= 1;
                if *n == 0 {
                    counts.workspaces.remove(&self.workspace);
                }
            }
        }
    }
}
