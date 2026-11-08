//! Live pictures of agents' browsers (designs/agent-computers 3 and 12). While someone
//! watches, the browser helper's `live` mode runs on the computer through the provider's
//! authenticated program channel; no port is opened. Its frames are shared with every
//! watcher, a person's input during a hand-over is passed to it, and it stops when the
//! last watcher leaves.
use super::{Awake, Call, Provider};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{Notify, broadcast, mpsc};

/// A new name for a live program on the computer, so one that is stopping never takes
/// the next one's input.
pub(crate) fn tag() -> String {
    format!("cuckoo-live-{}", uuid::Uuid::new_v4().simple())
}

/// Where to reach a computer, owned so a feed can outlive the request that started it.
pub(crate) struct Reach {
    pub provider: &'static dyn Provider,
    pub api: String,
    pub computers: String,
    pub key: String,
    pub awake: Awake,
}
impl Reach {
    fn call(&self) -> Call<'_> {
        Call {
            api: &self.api,
            computers: &self.computers,
            key: &self.key,
        }
    }
}

struct Feed {
    lines: broadcast::Sender<Arc<str>>,
    /// The latest frame, for people who start watching between frames.
    last: Arc<Mutex<Option<Arc<str>>>>,
    input: mpsc::Sender<String>,
    nobody: Arc<Notify>,
    watchers: usize,
}

/// Feeds by computer (provider id).
#[derive(Clone, Default)]
pub(crate) struct Feeds {
    inner: Arc<Mutex<HashMap<String, Feed>>>,
}

/// One watcher's view of a feed.
pub(crate) struct Watch {
    pub lines: broadcast::Receiver<Arc<str>>,
    pub last: Option<Arc<str>>,
}

impl Feeds {
    /// Joins the computer's feed, if it is running.
    pub(crate) fn join(&self, id: &str) -> Option<Watch> {
        let mut feeds = self.inner.lock().ok()?;
        let feed = feeds.get_mut(id)?;
        feed.watchers += 1;
        let last = feed.last.lock().ok().and_then(|l| l.clone());
        Some(Watch {
            lines: feed.lines.subscribe(),
            last,
        })
    }

    /// Starts sharing a just-started live program's output; the caller watches it.
    pub(crate) fn start(
        &self,
        id: &str,
        tag: String,
        reach: Reach,
        mut output: mpsc::Receiver<Vec<u8>>,
    ) -> Option<Watch> {
        let (lines, receiver) = broadcast::channel(16);
        let (input, mut inputs) = mpsc::channel::<String>(32);
        let nobody = Arc::new(Notify::new());
        let last = Arc::new(Mutex::new(None));
        {
            let mut feeds = self.inner.lock().ok()?;
            feeds.insert(
                id.to_owned(),
                Feed {
                    lines: lines.clone(),
                    last: last.clone(),
                    input,
                    nobody: nobody.clone(),
                    watchers: 1,
                },
            );
        }
        let feeds = self.clone();
        let id = id.to_owned();
        tokio::spawn(async move {
            let mut pending: Vec<u8> = Vec::new();
            loop {
                tokio::select! {
                    chunk = output.recv() => {
                        let Some(chunk) = chunk else { break };
                        pending.extend_from_slice(&chunk);
                        while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                            let line: Vec<u8> = pending.drain(..=end).collect();
                            let text: Arc<str> = String::from_utf8_lossy(&line[..line.len() - 1]).into();
                            if text.starts_with("{\"frame\"")
                                && let Ok(mut latest) = last.lock()
                            {
                                *latest = Some(text.clone());
                            }
                            let _ = lines.send(text);
                        }
                    }
                    line = inputs.recv() => {
                        let Some(line) = line else { break };
                        if let Err(error) = reach.provider.send(&reach.call(), &reach.awake, &tag, format!("{line}\n").as_bytes()).await {
                            tracing::warn!("A person's input could not reach the browser: {error}");
                        }
                    }
                    _ = nobody.notified() => {
                        let _ = reach.provider.send(&reach.call(), &reach.awake, &tag, b"{\"stop\":true}\n").await;
                        break;
                    }
                }
            }
            if let Ok(mut map) = feeds.inner.lock() {
                map.remove(&id);
            }
        });
        Some(Watch {
            lines: receiver,
            last: None,
        })
    }

    /// One watcher left; the live program stops when nobody watches.
    pub(crate) fn leave(&self, id: &str) {
        if let Ok(mut feeds) = self.inner.lock()
            && let Some(feed) = feeds.get_mut(id)
        {
            feed.watchers = feed.watchers.saturating_sub(1);
            if feed.watchers == 0 {
                feed.nobody.notify_one();
            }
        }
    }

    /// A person's input for the browser, when its live program runs.
    pub(crate) fn send(&self, id: &str, line: String) -> bool {
        let sender = self
            .inner
            .lock()
            .ok()
            .and_then(|feeds| feeds.get(id).map(|feed| feed.input.clone()));
        sender.is_some_and(|sender| sender.try_send(line).is_ok())
    }
}
