//! Live delivery hints only. Assignment ownership and replay live in PostgreSQL.
use super::model::Scope;
use axum::response::sse::Event;
use std::{
    collections::HashMap,
    convert::Infallible,
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::sync::{Notify, mpsc};
type Key = (String, String);
pub(super) type Receiver = mpsc::Receiver<Result<Event, Infallible>>;
struct Connection {
    id: String,
    sender: mpsc::Sender<Result<Event, Infallible>>,
    sent: HashMap<String, Instant>,
}
#[derive(Default)]
pub(super) struct Hub {
    connections: Mutex<HashMap<Key, Connection>>,
    pub wake: Notify,
}
impl Hub {
    pub fn close(&self) {
        self.connections.lock().unwrap().clear();
    }
    pub fn attach(&self, key: Key) -> anyhow::Result<(String, Receiver)> {
        let mut connections = self.connections.lock().unwrap();
        anyhow::ensure!(
            connections.contains_key(&key) || connections.len() < 64,
            "Local executor connection limit reached"
        );
        let id = uuid::Uuid::new_v4().to_string();
        let (sender, receiver) = mpsc::channel(32);
        connections.insert(
            key,
            Connection {
                id: id.clone(),
                sender,
                sent: HashMap::new(),
            },
        );
        self.wake.notify_one();
        Ok((id, receiver))
    }
    pub fn detach(&self, key: &Key, id: &str) {
        let mut connections = self.connections.lock().unwrap();
        if connections.get(key).is_some_and(|c| c.id == id) {
            connections.remove(key);
        }
    }
    pub fn connected(&self) -> Vec<Key> {
        self.connections
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, c)| !c.sender.is_closed())
            .map(|(k, _)| k.clone())
            .collect()
    }
    pub fn offer(&self, key: &Key, scope: &Scope) {
        let mut connections = self.connections.lock().unwrap();
        let Some(connection) = connections.get_mut(key) else {
            return;
        };
        if connection
            .sent
            .get(&scope.assignment)
            .is_some_and(|time| time.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        let Ok(event) = Event::default().event("assignment").json_data(scope) else {
            return;
        };
        if connection.sender.try_send(Ok(event)).is_ok() {
            connection
                .sent
                .retain(|_, time| time.elapsed() < Duration::from_secs(5));
            connection
                .sent
                .insert(scope.assignment.clone(), Instant::now());
        }
    }
}
