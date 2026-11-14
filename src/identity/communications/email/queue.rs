//! Reading agent mail: each controller long-polls the SQS queue SES notifies.
//! A received mail is read from S3, parsed and handed to the gateway's intake
//! once per recipient address on Cuckoo's domain; a delivery event settles the
//! reply it names. A queue message is deleted only after Cuckoo saved what it
//! said, so a crash leaves it to be read again, and intake drops repeats by
//! SES's mail ID. Messages that keep failing move to the queue's dead-letter
//! queue (an AWS redrive policy), never into an endless loop here.
use super::{
    inbound,
    notification::{self, Notice, Received},
    ses::EmailTransport,
    settings::{SERVER, Settings},
};
use crate::identity::postgres::{EmailIncoming, PgStore};
use anyhow::{Context, Result, ensure};
use object_store::{ObjectStore, ObjectStoreExt, aws::AmazonS3Builder, path::Path as ObjectPath};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

/// SES accepts mail up to 40 MB; Cuckoo reads text from the first 10 MB.
const MAX_MAIL_BYTES: usize = 10 * 1024 * 1024;
const IDLE: Duration = Duration::from_secs(60);

pub(crate) fn start(app: super::super::super::Shared) {
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let transport = EmailTransport::new();
        let mut buckets: Option<(String, Arc<dyn ObjectStore>)> = None;
        loop {
            let settings = match transport.settings() {
                Ok(Some(s)) => s,
                Ok(None) => {
                    tokio::select! { _ = stopping.cancelled() => break, _ = tokio::time::sleep(IDLE) => continue }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Email settings are invalid; agent mail is paused");
                    tokio::select! { _ = stopping.cancelled() => break, _ = tokio::time::sleep(IDLE) => continue }
                }
            };
            let bucket = match &buckets {
                Some((name, store)) if *name == settings.bucket => store.clone(),
                _ => match s3(&settings) {
                    Ok(store) => {
                        buckets = Some((settings.bucket.clone(), store.clone()));
                        store
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "The mail bucket is unavailable");
                        tokio::select! { _ = stopping.cancelled() => break, _ = tokio::time::sleep(IDLE) => continue }
                    }
                },
            };
            let round = tokio::select! {
                _ = stopping.cancelled() => break,
                round = read_once(&app.store, &transport, &settings, bucket.as_ref()) => round,
            };
            match round {
                Ok(0) => {}
                Ok(handled) => {
                    app.runtimes.chat_wake.notify_one();
                    tracing::info!(handled, "Agent mail");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "The mail queue could not be read; it will retry");
                    tokio::select! { _ = stopping.cancelled() => break, _ = tokio::time::sleep(Duration::from_secs(10)) => {} }
                }
            }
        }
    });
}

fn s3(settings: &Settings) -> Result<Arc<dyn ObjectStore>> {
    let store = AmazonS3Builder::from_env()
        .with_bucket_name(&settings.bucket)
        .with_region(&settings.region)
        .with_allow_http(false)
        .build()
        .context("Cannot configure the mail bucket")?;
    Ok(Arc::new(store))
}

/// One long poll: up to ten notifications, each handled and then deleted.
/// Returns how many were handled.
pub(crate) async fn read_once(
    store: &PgStore,
    transport: &EmailTransport,
    settings: &Settings,
    bucket: &dyn ObjectStore,
) -> Result<usize> {
    let answer = sqs(
        transport,
        settings,
        "ReceiveMessage",
        json!({"QueueUrl": settings.queue_url, "MaxNumberOfMessages": 10,
            "WaitTimeSeconds": if settings.api.is_some() { 0 } else { 20 }, "VisibilityTimeout": 120}),
    )
    .await?;
    let mut handled = 0;
    for message in answer["Messages"].as_array().cloned().unwrap_or_default() {
        let body = message["Body"].as_str().unwrap_or_default();
        match handle(store, settings, bucket, body).await {
            Ok(()) => {
                sqs(
                    transport,
                    settings,
                    "DeleteMessage",
                    json!({"QueueUrl": settings.queue_url, "ReceiptHandle": message["ReceiptHandle"]}),
                )
                .await?;
                handled += 1;
            }
            // Left on the queue: read again after its visibility timeout.
            Err(e) => tracing::warn!(error = %e, "A mail notification could not be handled"),
        }
    }
    Ok(handled)
}

async fn handle(
    store: &PgStore,
    settings: &Settings,
    bucket: &dyn ObjectStore,
    body: &str,
) -> Result<()> {
    match notification::parse(body)? {
        Notice::Received(received) => receive(store, settings, bucket, &received).await,
        Notice::Outcome {
            workspace,
            key,
            delivered,
            ..
        } => {
            let source = crate::identity::communications::adapters::Source {
                workspace,
                transport: "email".into(),
                server: SERVER.into(),
            };
            store.transport_receipt(&source, &key, delivered).await
        }
        Notice::Other => Ok(()),
    }
}

async fn receive(
    store: &PgStore,
    settings: &Settings,
    bucket: &dyn ObjectStore,
    received: &Received,
) -> Result<()> {
    let ours: Vec<&String> = received
        .recipients
        .iter()
        .filter(|r| settings.serves(r))
        .collect();
    if ours.is_empty() || received.unsafe_content {
        return Ok(());
    }
    ensure!(
        received.bucket == settings.bucket,
        "Mail stored in an unexpected bucket"
    );
    let object = bucket
        .get(&ObjectPath::from(received.key.as_str()))
        .await
        .context("The stored mail could not be read")?;
    if object.meta.size > MAX_MAIL_BYTES as u64 {
        tracing::warn!(
            size = object.meta.size,
            "A mail too large to read was dropped"
        );
        return Ok(());
    }
    let raw = object
        .bytes()
        .await
        .context("The stored mail could not be read")?;
    let Some(mail) = inbound::parse(&raw) else {
        return Ok(());
    };
    for to in ours {
        store
            .email_incoming(&EmailIncoming {
                event: received.ses_id.clone(),
                to: to.clone(),
                dmarc: received.dmarc,
                mail: &mail,
            })
            .await?;
    }
    Ok(())
}

/// One SQS JSON-protocol call, signed with the controller's role.
async fn sqs(
    transport: &EmailTransport,
    settings: &Settings,
    action: &str,
    body: Value,
) -> Result<Value> {
    let headers = BTreeMap::from([
        (
            "content-type".to_owned(),
            "application/x-amz-json-1.0".to_owned(),
        ),
        ("x-amz-target".to_owned(), format!("AmazonSQS.{action}")),
    ]);
    let (status, body) = transport
        .call(settings, "sqs", &settings.sqs(), headers, body.to_string())
        .await?
        .context("SQS could not be reached")?;
    ensure!((200..300).contains(&status), "SQS answered HTTP {status}");
    Ok(serde_json::from_slice(&body).unwrap_or(Value::Null))
}
