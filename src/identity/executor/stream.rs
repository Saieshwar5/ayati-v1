//! Bounded SSE consumption; reconnects replay durable offers without re-executing runs.
use crate::identity::worker_control::{
    client::{Client, Unavailable},
    model::Scope,
};
use anyhow::{Result, ensure};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
pub(super) enum Event {
    Offer(Box<Scope>),
    Denied,
}
pub(super) async fn listen(
    client: Client,
    path: String,
    sender: mpsc::Sender<Event>,
    stop: CancellationToken,
) {
    loop {
        let connected =
            tokio::select! {_=stop.cancelled()=>return,result=client.events(&path)=>result};
        match connected {
            Ok(response) => {
                let _ = read(response, &sender, &stop).await;
            }
            Err(error) if error.is::<Unavailable>() => {}
            Err(_) => {
                let _ = sender.send(Event::Denied).await;
                return;
            }
        }
        tokio::select! {_=stop.cancelled()=>return,_=tokio::time::sleep(Duration::from_millis(150))=>{}}
    }
}
async fn read(
    mut response: reqwest::Response,
    sender: &mpsc::Sender<Event>,
    stop: &CancellationToken,
) -> Result<()> {
    let mut buffer = Vec::new();
    let mut data = None;
    loop {
        let chunk = tokio::select! {_=stop.cancelled()=>return Ok(()),result=tokio::time::timeout(Duration::from_secs(3),response.chunk())=>result??};
        let Some(chunk) = chunk else {
            return Ok(());
        };
        buffer.extend_from_slice(&chunk);
        while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
            ensure!(index <= 16384, "Executor event exceeds limit");
            let line = buffer.drain(..=index).collect::<Vec<_>>();
            let line = std::str::from_utf8(&line)?.trim_end_matches(['\r', '\n']);
            if let Some(value) = line.strip_prefix("data:") {
                data = Some(value.trim().to_owned());
            }
            if line.is_empty()
                && let Some(data) = data.take()
            {
                let scope: Scope = serde_json::from_str(&data)?;
                tokio::select! {_=stop.cancelled()=>return Ok(()),result=sender.send(Event::Offer(Box::new(scope)))=>result?}
            }
        }
        ensure!(buffer.len() <= 16384, "Executor event exceeds limit");
    }
}
