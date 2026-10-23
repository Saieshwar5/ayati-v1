//! Persistent local executor with prepared, single-use Goose slots and durable dispatch.
mod harness;
pub(crate) mod model;
mod output;
#[cfg(test)]
mod output_tests;
mod slots;
mod stream;
use crate::identity::{
    provider::random_token,
    worker_config,
    worker_control::{
        client::Client,
        config,
        model::{Registration, Scope},
    },
};
use anyhow::{Context, Result, ensure};
use model::{Capacity, Profile};
use slots::{Control, Done, Slot, State};
use std::{path::Path, time::Duration};
use tokio::{sync::mpsc, task::JoinSet};
use tokio_util::sync::CancellationToken;

pub async fn serve(data: &Path) -> Result<()> {
    for name in [
        "RELAY_DATABASE_URL",
        "RELAY_SIMULATION_DATABASE_URL",
        "WORKOS_API_KEY",
        "WORK_OS_API_KEY",
    ] {
        ensure!(
            std::env::var_os(name).is_none(),
            "Executors must not receive {name}"
        );
    }
    let profile = Profile {
        provider: std::env::var("AYATI_GOOSE_PROVIDER").unwrap_or_else(|_| "fireworks-ai".into()),
        model: std::env::var("AYATI_MODEL").context("Executor requires AYATI_MODEL")?,
    };
    profile.validate()?;
    let size = worker_config::seconds("AYATI_EXECUTOR_SLOTS", 2, 1, 16)? as usize;
    let url = config::url(&std::env::var("AYATI_WORKER_CONTROL_URL")?)?;
    let key_file = std::path::PathBuf::from(
        std::env::var_os("AYATI_WORKER_CONTROL_KEY_FILE")
            .context("Executor requires its private local bootstrap file")?,
    );
    std::fs::create_dir_all(data)?;
    let _lock = super::adoption::lock(&data.join("executor.lock"))?;
    // After a crash no old session is reusable. The exclusive data-directory lock
    // and parent-death child boundary let this incarnation reclaim its scratch.
    let root = slots::root(data);
    if root.exists() {
        tokio::fs::remove_dir_all(&root)
            .await
            .context("Could not clear previous executor session state")?;
    }
    let registration = Registration {
        executor: uuid::Uuid::new_v4().to_string(),
        incarnation: uuid::Uuid::new_v4().to_string(),
        token: random_token(),
        pooled: true,
    };
    register(&url, &key_file, &registration).await?;
    let client = Client::new(&url, &registration.token)?;
    let base = format!(
        "/v1/executors/{}/{}/pool",
        registration.executor, registration.incarnation
    );
    let draining = CancellationToken::new();
    let signal = draining.clone();
    let signal_task = tokio::spawn(async move {
        super::worker::shutdown_signal().await;
        signal.cancel();
    });
    let stream_stop = CancellationToken::new();
    let fatal = CancellationToken::new();
    let control = Control {
        client: client.clone(),
        registration: registration.clone(),
        url: url.clone(),
        stop: fatal.clone(),
    };
    let preparation = CancellationToken::new();
    let (sender, mut events) = mpsc::channel(32);
    let listener = tokio::spawn(stream::listen(
        client.clone(),
        format!("{base}/events"),
        sender,
        stream_stop.clone(),
    ));
    let mut slots = (0..size)
        .map(|_| Slot::new(profile.clone()))
        .collect::<Vec<_>>();
    let mut jobs = JoinSet::new();
    for (index, slot) in slots.iter().enumerate() {
        slot.prepare(index, &root, &preparation, &mut jobs);
    }
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut drained = false;
    let mut failed = false;
    let mut refresh = tokio::time::Instant::now() + Duration::from_secs(300);
    tracing::info!(executor=%registration.executor,slots=size,"Persistent executor preparing harness slots");
    loop {
        tokio::select! {
            _ = draining.cancelled(), if !drained => {
                preparation.cancel();
                match client.retry::<Vec<Scope>>(&format!("{base}/drain"), &serde_json::json!({}), 5).await {
                    Ok(offers) => {
                        for scope in offers {
                            handle_offer(&mut slots, &mut jobs, &control, scope).await;
                        }
                    },
                    Err(_) => {
                        failed = true;
                        fatal.cancel();
                    }
                }
                drained = true;
                tracing::info!("Executor draining; no new assignments admitted");
            },
            event = events.recv() => match event {
                Some(stream::Event::Offer(scope)) => handle_offer(&mut slots, &mut jobs, &control, *scope).await,
                Some(stream::Event::Denied) | None => {
                    failed = true;
                    fatal.cancel();
                    draining.cancel();
                    preparation.cancel();
                    drained = true;
                }
            },
            event = jobs.join_next(), if !jobs.is_empty() => {
                match event {
                    Some(Ok(Done::Prepared(index, generation, result))) if slots[index].generation == generation => {
                        match result {
                            Ok(harness) if !drained => {
                                slots[index].pid = Some(harness.pid);
                                slots[index].state = State::Ready(Box::new(harness));
                            },
                            Ok(harness) => {
                                harness.close().await;
                                slots[index].state = State::Closed;
                            },
                            Err(_) if !drained => {
                                tracing::warn!(slot=%slots[index].id, "Harness preparation failed; slot remains unavailable");
                                slots[index].prepare(index, &root, &preparation, &mut jobs);
                            },
                            Err(_) => slots[index].state = State::Closed,
                        }
                    },
                    Some(Ok(Done::Finished(index, generation))) if slots[index].generation == generation => {
                        slots[index].next();
                        if !drained {
                            slots[index].prepare(index, &root, &preparation, &mut jobs);
                        } else {
                            slots[index].state = State::Closed;
                        }
                    },
                    Some(Err(_)) => {
                        failed = true;
                        fatal.cancel();
                        draining.cancel();
                        preparation.cancel();
                        drained = true;
                    },
                    _ => {},
                }
            },
            _ = tick.tick() => {
                if !drained {
                    for (index, slot) in slots.iter_mut().enumerate() {
                        if let State::Ready(harness) = &slot.state && !harness.ready().await {
                            if let State::Ready(harness) = std::mem::replace(&mut slot.state, State::Closed) {
                                harness.close().await;
                            }
                            slot.next();
                            slot.prepare(index, &root, &preparation, &mut jobs);
                        }
                    }
                }
                if tokio::time::Instant::now() >= refresh && register(&url, &key_file, &registration).await.is_ok() {
                    refresh = tokio::time::Instant::now() + Duration::from_secs(300);
                }
            },
        }
        let report = Capacity {
            slots: slots.iter().filter_map(Slot::report).collect(),
        };
        if let Err(error) = client
            .post::<serde_json::Value>(&format!("{base}/capacity"), &report)
            .await
        {
            if error.is::<super::worker_control::client::Conflict>() {
                tracing::warn!(
                    "Readiness transition deferred until the previous lease is reconciled"
                );
            } else if !error.is::<super::worker_control::client::Unavailable>() {
                failed = true;
                fatal.cancel();
                draining.cancel();
                preparation.cancel();
                drained = true;
            }
        }
        // A panicked task has no Finished event to clear its local Running state.
        // Once all tasks have stopped, stale bookkeeping must not hang fatal drain.
        if drained
            && ((!slots
                .iter()
                .any(|slot| matches!(slot.state, State::Running(_))))
                || (failed && jobs.is_empty()))
        {
            preparation.cancel();
            slots::close(&mut slots).await;
            while let Some(event) = jobs.join_next().await {
                if let Ok(Done::Prepared(_, _, Ok(harness))) = event {
                    harness.close().await;
                }
            }
            break;
        }
    }
    let _: Result<serde_json::Value> = client
        .post(&format!("{base}/stop"), &serde_json::json!({}))
        .await;
    stream_stop.cancel();
    listener.abort();
    signal_task.abort();
    ensure!(
        !failed,
        "Executor stopped after losing its authenticated control boundary"
    );
    Ok(())
}
async fn handle_offer(
    slots: &mut [Slot],
    jobs: &mut JoinSet<Done>,
    control: &Control,
    scope: Scope,
) {
    if scope.executor != control.registration.executor
        || scope.incarnation != control.registration.incarnation
    {
        return;
    }
    let Some(index) = slots.iter().position(|slot| slot.id == scope.slot) else {
        return;
    };
    let slot = &mut slots[index];
    if let State::Running(current) = &slot.state
        && **current == scope
    {
        return;
    }
    if slot.generation == scope.slot_generation && matches!(slot.state, State::Ready(_)) {
        slot.start(index, scope, control, jobs);
    } else if !matches!(slot.state, State::Running(_)) {
        let _ = control
            .client
            .release(&control.registration, &scope, "lost")
            .await;
    }
}
async fn register(url: &str, key_file: &Path, registration: &Registration) -> Result<()> {
    let key = config::key(key_file)?;
    let _: serde_json::Value = Client::new(url, &key)?
        .retry("/v1/executors/register", registration, 5)
        .await?;
    Ok(())
}
