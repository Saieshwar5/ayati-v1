//! Speech-to-text for voice notes and other audio attachments (schema 95).
//!
//! With `AYATI_TRANSCRIPTION_MODEL`, an audio upload queues a transcript and
//! every controller runs this worker. A database lease decides which controller
//! sends each recording, so replicas never both send the same audio; tries are
//! bounded and each request has a time limit. The audio goes to the provider;
//! neither it, its transcript nor the key is ever logged.
mod length;
pub(crate) mod request;
#[cfg(test)]
mod request_tests;
#[cfg(test)]
mod worker_tests;

use super::{
    Shared,
    collaboration::Object,
    postgres::{
        ModelCall,
        transcripts::{Claimed, Outcome},
    },
};
use anyhow::{Context, Result};
use std::time::{Duration, Instant};

/// Recordings are limited to five minutes; a few seconds over allows for rounding.
const MAX_DURATION_MS: i64 = 305_000;
/// The upload limit; nothing larger is ever sent.
const MAX_BYTES: i64 = 10 * 1024 * 1024;
/// Transcripts one controller works on at once.
const AT_ONCE: i64 = 4;
/// The provider's time limit for one recording, uploading included. The lease
/// (`transcripts::LEASE_MS`) is longer, so a slow request can't be taken twice.
const TIME_LIMIT: Duration = Duration::from_secs(90);

/// An upload here wakes the worker at once, and one on another controller is
/// announced through PostgreSQL (schema 97). Retries and expired leases are due
/// at a known time, so the worker sleeps until then; it still looks this often
/// in case a notification was lost. Until November 2026 it looked every 5 s.
const FALLBACK: Duration = Duration::from_secs(60);
/// After the queue was unreadable, look again this soon.
const RETRY: Duration = Duration::from_secs(5);

static WAKE: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Starts this controller's worker now rather than at its next look.
pub(crate) fn wake() {
    WAKE.notify_one();
}

/// The controller's transcription model, from its configuration.
#[derive(Clone)]
pub(crate) struct Configured {
    pub file: std::path::PathBuf,
    pub provider: String,
    pub model: String,
}
impl Configured {
    pub fn from(config: &crate::config::Controller) -> Option<Self> {
        let (provider, model) = config.transcription_model.clone()?;
        Some(Self {
            file: config.model_providers_file.clone()?,
            provider,
            model,
        })
    }
    fn settings(&self) -> request::Settings<'_> {
        request::Settings {
            file: &self.file,
            provider: &self.provider,
            model: &self.model,
            time_limit: TIME_LIMIT,
        }
    }
}

/// Checks the provider exists in the providers file with a readable key, then
/// starts the worker. A problem stops startup, so it is seen at once.
pub(in crate::identity) fn start(app: Shared) -> Result<()> {
    let Some(configured) = Configured::from(&app.config) else {
        return Ok(());
    };
    super::worker_control::model_provider(&configured.file, &configured.provider)
        .context("AYATI_TRANSCRIPTION_MODEL names a provider Cuckoo can't use")?;
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let work = app.store.work.clone();
        loop {
            let busy = round(&app, &configured).await;
            let pause = if busy {
                Duration::ZERO
            } else {
                match app.store.next_transcript_in().await {
                    Ok(next) => next.map_or(FALLBACK, |next| next.min(FALLBACK)),
                    Err(_) => RETRY,
                }
            };
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = WAKE.notified() => {}
                _ = work.transcribe.notified() => {}
                _ = tokio::time::sleep(pause) => {}
            }
        }
    });
    Ok(())
}

/// Takes a few waiting transcripts and works on them together; true when
/// there were any.
pub(crate) async fn round(app: &Shared, configured: &Configured) -> bool {
    let claimed = match app.store.claim_transcripts(AT_ONCE).await {
        Ok(claimed) => claimed,
        Err(_) => {
            tracing::warn!("Voice transcription queue unavailable");
            return false;
        }
    };
    let finished = claimed.iter().map(|item| async move {
        let outcome = transcribe(app, configured, item).await;
        if app.store.finish_transcript(item, outcome).await.is_err() {
            tracing::warn!("Voice transcript could not be saved; it will be tried again");
        }
    });
    futures_util::future::join_all(finished).await;
    !claimed.is_empty()
}

async fn transcribe(app: &Shared, configured: &Configured, item: &Claimed) -> Outcome {
    if item.size > MAX_BYTES {
        return Outcome::Failed { duration_ms: None };
    }
    let object = Object {
        id: item.object.clone(),
        name: item.name.clone(),
        mime: item.mime.clone(),
        size: item.size,
        checksum: item.checksum.clone(),
    };
    let Ok(bytes) = super::collaboration::stored_object(app, &item.workspace, &object).await else {
        tracing::warn!("Voice note unavailable from file storage; it will be tried again");
        return Outcome::Retry;
    };
    let measured = length::duration_ms(&item.mime, &bytes);
    let whole_ms = |ms: Option<i64>| ms.and_then(|ms| i32::try_from(ms).ok());
    if measured.is_some_and(|ms| ms > MAX_DURATION_MS) {
        return Outcome::Failed {
            duration_ms: whole_ms(measured),
        };
    }
    let started = Instant::now();
    let answer = request::transcribe(
        &configured.settings(),
        &item.name,
        &item.mime,
        bytes.to_vec(),
    )
    .await;
    let latency_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    match answer {
        Ok(heard) => {
            // A model call like any other, so operators see its count and speed.
            // Providers price audio by the minute, which tokens don't capture.
            let usage = app
                .store
                .record_model_usage(&ModelCall {
                    workspace: &item.workspace,
                    assignment: "transcription",
                    agent: "",
                    job: None,
                    provider: &configured.provider,
                    model: &configured.model,
                    provider_model: &configured.model,
                    attempts: 1,
                    input_tokens: 0,
                    output_tokens: 0,
                    person: Some(&item.uploader),
                    charge_micro: 0,
                    cost_micro: 0,
                    latency_ms: Some(latency_ms),
                })
                .await;
            if usage.is_err() {
                tracing::warn!("Voice transcription usage could not be recorded");
            }
            Outcome::Heard {
                text: heard.text,
                duration_ms: whole_ms(measured.or(heard.duration_ms)),
                model: configured.model.clone(),
            }
        }
        Err(failure) => {
            tracing::warn!(
                status = failure.status,
                retry = failure.retry,
                "Voice transcription failed"
            );
            let _ = app
                .store
                .record_model_failure(
                    &item.workspace,
                    &configured.provider,
                    &configured.model,
                    failure.status,
                )
                .await;
            if failure.retry {
                Outcome::Retry
            } else {
                Outcome::Failed {
                    duration_ms: whole_ms(measured),
                }
            }
        }
    }
}
