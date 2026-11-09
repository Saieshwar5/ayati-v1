//! Automatic checks of unconfirmed writes, every 15 seconds on each controller:
//! ask the provider whether a write it didn't confirm happened (schema 107). Found
//! settles it; anything else is recorded for the person who answers. Only reads
//! the app; nothing is ever sent again. Several controllers can run this at once
//! (`postgres::control_actions::outcomes::rechecks` leases each check).
use crate::identity::{
    connectors::{ConnectorProvider, Found, Saved},
    postgres::{
        PgStore,
        control_actions::outcomes::rechecks::{Asked, Due},
    },
};
use anyhow::Result;
use std::{sync::Arc, time::Duration};

const EVERY: Duration = Duration::from_secs(15);
const BATCH: i64 = 20;
/// Within the check's lease (120 s), so another controller never asks at once.
const ASK: Duration = Duration::from_secs(30);

/// Finds the provider a saved account uses.
pub(crate) type Lookup = dyn Fn(&str) -> Result<Arc<dyn ConnectorProvider>> + Sync;

pub(in crate::identity) fn start(app: crate::identity::Shared) {
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let mut tick = tokio::time::interval(EVERY);
        loop {
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tick.tick() => {}
            }
            let round = run_once(&app.store, &crate::identity::connectors::named).await;
            if round.checked > 0 {
                tracing::info!(round.checked, round.settled, "Unconfirmed write checks");
            }
        }
    });
}

/// What one round did, for logs and tests.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Round {
    pub checked: usize,
    pub settled: usize,
}

pub(crate) async fn run_once(store: &PgStore, lookup: &Lookup) -> Round {
    let mut round = Round::default();
    let Ok(due) = store.outcome_checks_due(BATCH).await else {
        return round;
    };
    for check in due {
        let Some(asked) = ask(store, &check, lookup).await else {
            continue;
        };
        if matches!(asked, Asked::Found(_)) {
            round.checked += 1;
        }
        match store.outcome_check_done(&check, &asked).await {
            Ok(true) => round.settled += 1,
            Ok(false) => {}
            // The lease runs out and the check is made again.
            Err(_) => {
                tracing::warn!("An unconfirmed write check could not be saved; it will retry")
            }
        }
    }
    round
}

/// `None` leaves the check for later: its provider was switched off meanwhile.
async fn ask(store: &PgStore, due: &Due, lookup: &Lookup) -> Option<Asked> {
    if !matches!(
        store.connector_provider_paused(&due.provider).await,
        Ok(None)
    ) {
        return None;
    }
    let Ok(provider) = lookup(&due.provider) else {
        return Some(Asked::Found(Found::Unknown));
    };
    if !provider.capabilities().find_outcome {
        return Some(Asked::CannotCheck);
    }
    let Some(operation) = &due.operation else {
        return Some(Asked::Found(Found::Unknown));
    };
    let saved = Saved {
        workspace: &due.workspace,
        server: &due.server,
        binding: due.binding.as_ref(),
        fingerprint: &due.fingerprint,
    };
    // The key is the action id, the same one the call was given.
    let found = tokio::time::timeout(
        ASK,
        provider.find_outcome(&saved, operation, &due.action, &due.arguments),
    )
    .await;
    Some(Asked::Found(match found {
        Ok(Ok(found)) => found,
        _ => Found::Unknown,
    }))
}
