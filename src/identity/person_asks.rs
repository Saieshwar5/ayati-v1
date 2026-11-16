//! An agent waiting for a person it asked continues without an answer after a
//! day (schema 124): every minute, waits past their time end, which resumes
//! those tasks (`agent_teamwork.rs`).
use crate::identity::Shared;
use std::time::Duration;

pub(in crate::identity) fn start(app: Shared) {
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        loop {
            if let Err(error) = app.store.expire_person_asks().await {
                tracing::warn!(reason = %error, "Waits for people could not be checked");
            }
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            }
        }
    });
}
