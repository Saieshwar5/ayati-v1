//! A local stand-in for Telegram's Bot API and helpers for the Telegram
//! PostgreSQL tests: queued updates per bot, every request each bot made,
//! and a workspace whose owner connected Telegram and created a bot.
use super::super::{tests::*, *};
use crate::identity::communications::telegram::{poller, settings::Settings};
use axum::{
    Json, Router,
    extract::{Path as RoutePath, State},
    routing::post,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

pub(super) const MANAGER: &str = "CuckooAgentsBot";
pub(super) const OWNER_TG: i64 = 1001;
pub(super) const MEMBER_TG: i64 = 1002;
pub(super) const STRANGER_TG: i64 = 3003;
pub(super) fn token(bot: i64) -> String {
    format!("{bot}:{}", "F".repeat(30))
}
pub(super) const MANAGER_ID: i64 = 999;

/// A local stand-in for Telegram's Bot API: queued updates per bot and every
/// request each bot made.
#[derive(Clone, Default)]
pub(super) struct Fake {
    pub updates: Arc<Mutex<HashMap<i64, Vec<Value>>>>,
    pub calls: Arc<Mutex<Vec<(i64, String, Value)>>>,
    pub refuse_html: Arc<Mutex<bool>>,
    pub blocked: Arc<Mutex<bool>>,
    pub next_id: Arc<Mutex<i64>>,
}
impl Fake {
    pub(super) fn push(&self, bot: i64, update: Value) {
        let mut id = self.next_id.lock().unwrap();
        *id += 1;
        let mut update = update;
        update["update_id"] = json!(*id);
        self.updates
            .lock()
            .unwrap()
            .entry(bot)
            .or_default()
            .push(update);
    }
    pub(super) fn message(&self, bot: i64, from: i64, chat: (i64, &str), text: Option<&str>) {
        let mut message = json!({"message_id": 1, "from": {"id": from, "is_bot": false, "first_name": "P", "username": format!("p{from}")},
            "chat": {"id": chat.0, "type": chat.1}});
        if let Some(text) = text {
            message["text"] = json!(text);
        }
        self.push(bot, json!({"message": message}));
    }
    pub(super) fn calls(&self, bot: i64, method: &str) -> Vec<Value> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(b, m, _)| *b == bot && m == method)
            .map(|(_, _, v)| v.clone())
            .collect()
    }
    pub(super) fn texts(&self, bot: i64, chat: i64) -> Vec<String> {
        self.calls(bot, "sendMessage")
            .into_iter()
            .filter(|v| v["chat_id"] == chat)
            .map(|v| v["text"].as_str().unwrap().to_owned())
            .collect()
    }
}
async fn api(
    State(fake): State<Fake>,
    RoutePath((bot, method)): RoutePath<(String, String)>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let bot: i64 = bot
        .trim_start_matches("bot")
        .split(':')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    fake.calls
        .lock()
        .unwrap()
        .push((bot, method.clone(), body.clone()));
    Json(match method.as_str() {
        "getUpdates" => {
            let offset = body["offset"].as_i64().unwrap_or(0);
            let all = fake
                .updates
                .lock()
                .unwrap()
                .get(&bot)
                .cloned()
                .unwrap_or_default();
            json!({"ok": true, "result": all.into_iter().filter(|u| u["update_id"].as_i64().unwrap() >= offset).collect::<Vec<_>>()})
        }
        "getManagedBotToken" => {
            json!({"ok": true, "result": token(body["user_id"].as_i64().unwrap())})
        }
        "sendMessage" if *fake.blocked.lock().unwrap() && bot != MANAGER_ID => {
            json!({"ok": false, "error_code": 403, "description": "Forbidden: bot was blocked by the user"})
        }
        "sendMessage" if *fake.refuse_html.lock().unwrap() && body["parse_mode"] == "HTML" => {
            json!({"ok": false, "error_code": 400, "description": "Bad Request: can't parse entities"})
        }
        "sendMessage" => json!({"ok": true, "result": {"message_id": 77}}),
        _ => json!({"ok": true, "result": true}),
    })
}
fn private(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}
pub(super) struct Harness {
    pub fake: Fake,
    pub settings: Settings,
    pub file: std::path::PathBuf,
    _dir: tempfile::TempDir,
}
pub(super) async fn harness() -> Harness {
    let fake = Fake::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/{bot}/{method}", post(api))
        .with_state(fake.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let manager = private(dir.path(), "manager", &format!("{}\n", token(MANAGER_ID)));
    let file = private(
        dir.path(),
        "telegram.json",
        &json!({"manager_username": MANAGER, "manager_token_file": manager, "api": base})
            .to_string(),
    );
    let settings = crate::identity::communications::telegram::settings::read(&file).unwrap();
    Harness {
        fake,
        settings,
        file,
        _dir: dir,
    }
}
impl Harness {
    /// Presses Start on the manager bot with a code from a Start link.
    pub(super) async fn start(&self, db: &PgStore, link: &str, from: i64) {
        let code = link.split("?start=").nth(1).unwrap();
        self.fake.message(
            MANAGER_ID,
            from,
            (from, "private"),
            Some(&format!("/start {code}")),
        );
        poller::run_once(db, &self.settings).await;
    }
    /// The owner taps "Create" in Telegram: the manager bot is told about the new bot.
    pub(super) async fn create(
        &self,
        db: &PgStore,
        creator: i64,
        bot: i64,
        username: &str,
    ) -> poller::Round {
        self.fake.push(MANAGER_ID, json!({"managed_bot": {"user": {"id": creator, "is_bot": false}, "bot": {"id": bot, "is_bot": true, "username": username}}}));
        poller::run_once(db, &self.settings).await
    }
}
pub(super) fn link(code: &str) -> String {
    format!("https://t.me/{MANAGER}?start={code}")
}
/// A workspace whose owner connected Telegram, with one agent whose bot exists.
pub(super) async fn connected(
    db: &PgStore,
    h: &Harness,
    name: &str,
    bot: i64,
) -> (User, String, String) {
    let owner = user(db, name).await;
    let w = workspace(db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(db, &owner, &w).await;
    h.start(
        db,
        &link(&db.telegram_connect_code(&owner, &w).await.unwrap()),
        OWNER_TG + bot,
    )
    .await;
    let on = db
        .telegram_activate(&owner, &w, &nova, MANAGER)
        .await
        .unwrap();
    assert_eq!(on.view["state"], "requested");
    let suggested = query(
        &db.db,
        "SELECT suggested FROM relay_telegram_bots WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<String>("", "suggested")
    .unwrap();
    assert_eq!(
        h.create(db, OWNER_TG + bot, bot, &suggested).await.created,
        1
    );
    (owner, w, nova)
}
