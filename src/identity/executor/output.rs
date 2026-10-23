//! Bounded ACP forwarding. A lag, gap or denied write cancels instead of losing text.
use crate::{
    agent::Event,
    identity::worker_control::{client::Client, model::AssignedRequest, output::Update},
};
use anyhow::{Context, Result, ensure};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

pub(super) struct Output {
    pub client: Client,
    pub assignment: AssignedRequest,
}
impl Output {
    pub async fn forward(
        self,
        mut events: broadcast::Receiver<Event>,
        reply: String,
        done: CancellationToken,
        cancel: CancellationToken,
    ) -> Result<()> {
        let result = self.consume(&mut events, &reply, &done, &cancel).await;
        if result.is_err() {
            cancel.cancel();
        }
        result
    }
    async fn consume(
        &self,
        events: &mut broadcast::Receiver<Event>,
        reply: &str,
        done: &CancellationToken,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let mut sequence = 0;
        let mut pending = String::new();
        let mut offset = 0;
        let mut total = 0;
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let event = tokio::select! {
                biased;
                _ = cancel.cancelled() => anyhow::bail!("Output stopped with its assignment"),
                event = events.recv() => Some(event?),
                _ = done.cancelled() => {
                    match events.try_recv() {
                        Ok(event) => Some(event),
                        Err(broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed) => {
                            self.flush(&mut pending, &mut sequence).await?;
                            return Ok(());
                        },
                        Err(_) => anyhow::bail!("Output receiver lagged"),
                    }
                },
                _ = tick.tick() => {
                    self.flush(&mut pending, &mut sequence).await?;
                    None
                },
            };
            if let Some(event) = event
                && event.message_id.as_deref() == Some(reply)
            {
                if event.kind == "prompt" {
                    ensure!(sequence == 0 && total == 0, "Repeated harness handoff");
                    self.send(1, "waiting", String::new()).await?;
                    sequence = 1;
                } else if event.kind == "delta" {
                    let text = event.text.context("Missing output text")?;
                    ensure!(event.offset == Some(offset), "Output offset gap");
                    offset += text.encode_utf16().count();
                    total += text.len();
                    ensure!(total <= 65536, "Reply output exceeds limit");
                    pending.push_str(&text);
                    if pending.len() >= 4096 {
                        self.flush(&mut pending, &mut sequence).await?;
                    }
                }
            }
        }
    }
    async fn flush(&self, pending: &mut String, sequence: &mut i64) -> Result<()> {
        while !pending.is_empty() {
            let mut length = pending.len().min(4096);
            while !pending.is_char_boundary(length) {
                length -= 1;
            }
            let text = pending[..length].to_owned();
            let next = *sequence + 1;
            self.send(next, "responding", text).await?;
            pending.drain(..length);
            *sequence = next;
        }
        Ok(())
    }
    async fn send(&self, sequence: i64, phase: &str, text: String) -> Result<()> {
        let update = Update {
            assignment: self.assignment.clone(),
            sequence,
            phase: phase.into(),
            text,
        };
        update.validate()?;
        let path = format!(
            "/v1/assignments/{}/output",
            self.assignment.scope.assignment
        );
        let reply: serde_json::Value = self
            .client
            .retry(&path, &update, 3)
            .await
            .context("Output was not acknowledged")?;
        ensure!(
            reply["sequence"].as_i64() == Some(sequence),
            "Unexpected output acknowledgment"
        );
        Ok(())
    }
}
