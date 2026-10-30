//! Publishes pool and database load once per interval (D226). In JSON log mode
//! each snapshot is one CloudWatch embedded metric format line, which the ECS
//! `awslogs` driver turns into metrics for autoscaling and alarms. Locally it is
//! a readable log line. Every controller publishes the same global figures, so
//! use the Maximum statistic. Counts and sizes only, never contents.
use super::{Shared, postgres::PoolSnapshot};
use serde_json::{Value, json};
use std::time::Duration;

const NAMESPACE: &str = "Ayati";
/// Table sizes change slowly; log them every this many snapshots.
const TABLES_EVERY: u64 = 10;

pub(super) fn start(app: Shared) {
    let seconds = app.config.metrics_seconds;
    if seconds == 0 {
        return;
    }
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let mut tick = tokio::time::interval(Duration::from_secs(seconds));
        for round in 0u64.. {
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tick.tick() => {}
            }
            match app.store.pool_snapshot().await {
                Ok(snapshot) if app.config.log_json => println!("{}", emf(&snapshot, now_ms())),
                Ok(snapshot) => tracing::info!(?snapshot, "Pool metrics"),
                Err(_) => tracing::warn!("Pool metrics unavailable"),
            }
            if round % TABLES_EVERY == 0
                && let Ok(tables) = app.store.largest_tables(10).await
            {
                let tables: serde_json::Map<String, Value> = tables
                    .into_iter()
                    .map(|(name, bytes)| (name, json!(bytes)))
                    .collect();
                let tables = Value::Object(tables);
                tracing::info!(tables = %tables, "Largest tables in bytes");
            }
        }
    });
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis())
}

/// One embedded metric format document. Names reference numeric top-level members.
fn emf(snapshot: &PoolSnapshot, timestamp_ms: u128) -> Value {
    let metrics = [
        ("QueuedWork", "Count", json!(snapshot.queued)),
        (
            "OldestQueuedSeconds",
            "Seconds",
            json!(snapshot.oldest_queued_seconds),
        ),
        ("ReadySlots", "Count", json!(snapshot.ready_slots)),
        ("BusySlots", "Count", json!(snapshot.busy_slots)),
        ("Executors", "Count", json!(snapshot.executors)),
        ("AdmissionWaits", "Count", json!(snapshot.admission_waits)),
        (
            "AdmissionWaitAverage",
            "Milliseconds",
            json!(snapshot.admission_wait_avg_ms),
        ),
        (
            "AdmissionWaitMax",
            "Milliseconds",
            json!(snapshot.admission_wait_max_ms),
        ),
        (
            "DatabaseConnections",
            "Count",
            json!(snapshot.database_connections),
        ),
        ("DatabaseBytes", "Bytes", json!(snapshot.database_bytes)),
    ];
    let mut document = json!({
        "_aws": {
            "Timestamp": timestamp_ms as u64,
            "CloudWatchMetrics": [{
                "Namespace": NAMESPACE,
                "Dimensions": [[]],
                "Metrics": metrics.iter().map(|(name, unit, _)| json!({"Name": name, "Unit": unit})).collect::<Vec<_>>(),
            }],
        },
        "message": "Pool metrics",
        "role": "controller",
    });
    for (name, _, value) in metrics {
        document[name] = value;
    }
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_snapshot_is_a_valid_embedded_metric_document() {
        let snapshot = PoolSnapshot {
            queued: 3,
            oldest_queued_seconds: 12,
            ready_slots: 4,
            busy_slots: 2,
            executors: 2,
            admission_waits: 5,
            admission_wait_avg_ms: 0.4,
            admission_wait_max_ms: 1.5,
            database_connections: 9,
            database_bytes: 1 << 20,
        };
        let document = emf(&snapshot, 1_793_000_000_000);
        let aws = &document["_aws"];
        assert_eq!(aws["Timestamp"], 1_793_000_000_000u64);
        let directive = &aws["CloudWatchMetrics"][0];
        assert_eq!(directive["Namespace"], NAMESPACE);
        assert_eq!(directive["Dimensions"], json!([[]]));
        let metrics = directive["Metrics"].as_array().unwrap();
        assert!(!metrics.is_empty() && metrics.len() <= 100);
        for metric in metrics {
            let name = metric["Name"].as_str().unwrap();
            assert!(
                document[name].is_number(),
                "{name} must be a numeric top-level member"
            );
        }
        assert_eq!(document["QueuedWork"], 3);
        // The document is one line of JSON, as CloudWatch Logs requires.
        assert!(!document.to_string().contains('\n'));
    }
}
