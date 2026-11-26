//! A Library file transfer holds a whole file (up to 25 MiB) in the controller's
//! memory, so only a few run at once. They were four for every workspace
//! together, taken before checking who asked and held while a slow client read:
//! four stalled connections stopped every workspace's uploads and downloads.
//! Now one workspace may use at most [`PER_WORKSPACE`] of them, a slot is taken
//! only after sign-in and access are checked, and the handlers end transfers
//! that stall (`files.rs`).
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Transfers at once on this controller. Bounded by memory: the hosted
/// controller has 512 MiB by default (deploy/aws/stacks/services.yaml).
pub(in crate::identity) const TOTAL: usize = 4;
/// Transfers at once for one workspace, so others always have room.
pub(in crate::identity) const PER_WORKSPACE: usize = 2;

pub(in crate::identity) struct Transfers {
    total: Arc<Semaphore>,
    busy: Arc<Mutex<HashMap<String, usize>>>,
}

impl Default for Transfers {
    fn default() -> Self {
        Self {
            total: Arc::new(Semaphore::new(TOTAL)),
            busy: Arc::default(),
        }
    }
}

/// One running transfer; dropping it frees its place.
pub(in crate::identity) struct Slot {
    _total: OwnedSemaphorePermit,
    workspace: String,
    busy: Arc<Mutex<HashMap<String, usize>>>,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut busy = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = busy.get_mut(&self.workspace) {
            *n -= 1;
            if *n == 0 {
                busy.remove(&self.workspace);
            }
        }
    }
}

impl Transfers {
    /// A place for one transfer in `workspace`, or why there is none now.
    pub(in crate::identity) fn take(&self, workspace: &str) -> Result<Slot, &'static str> {
        let mut busy = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let running = busy.get(workspace).copied().unwrap_or(0);
        if running >= PER_WORKSPACE {
            return Err(
                "Two file transfers are running in this workspace. Retry when one finishes.",
            );
        }
        let permit = self
            .total
            .clone()
            .try_acquire_owned()
            .map_err(|_| "File transfers are busy. Retry shortly.")?;
        busy.insert(workspace.to_owned(), running + 1);
        Ok(Slot {
            _total: permit,
            workspace: workspace.to_owned(),
            busy: self.busy.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_workspace_cannot_take_every_transfer() {
        let transfers = Transfers::default();
        let busy: Vec<Slot> = (0..PER_WORKSPACE)
            .map(|_| transfers.take("busy").unwrap())
            .collect();
        assert!(transfers.take("busy").is_err(), "the busy workspace waits");
        let other: Vec<Slot> = (0..TOTAL - PER_WORKSPACE)
            .map(|_| transfers.take("other").unwrap())
            .collect();
        assert_eq!(
            transfers.take("third").err(),
            Some("File transfers are busy. Retry shortly."),
            "all places are taken"
        );
        drop(busy);
        assert!(
            transfers.take("third").is_ok(),
            "a finished transfer frees its place"
        );
        drop(other);
        assert!(
            transfers.busy.lock().unwrap().len() <= 1,
            "finished workspaces are forgotten"
        );
    }
}
