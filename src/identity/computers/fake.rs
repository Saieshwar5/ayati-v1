//! A local stand-in for E2B's API, for tests only: each computer is a folder, its
//! commands really run with bash in that folder, and its files are files there. It
//! speaks the same requests as E2B, so tests exercise the real adapter.
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, post},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Computer {
    home: PathBuf,
    token: String,
    asleep: bool,
    network: Value,
}

#[derive(Clone, Default)]
struct Shared {
    root: PathBuf,
    computers: Arc<Mutex<HashMap<String, Computer>>>,
    log: Arc<Mutex<Vec<String>>>,
    next: Arc<Mutex<u32>>,
    /// Input of programs started with a tag, by "computer/tag".
    inputs: Arc<tokio::sync::Mutex<HashMap<String, tokio::process::ChildStdin>>>,
}

/// A running stand-in; dropping it stops nothing, the test process ends it.
pub(crate) struct Fake {
    pub url: String,
    _root: tempfile::TempDir,
    shared: Shared,
}
impl Fake {
    /// What was asked, in order ("create cuckoo-small", "pause fake1", ...).
    pub fn log(&self) -> Vec<String> {
        self.shared.log.lock().unwrap().clone()
    }
    pub fn asleep(&self, id: &str) -> Option<bool> {
        self.shared
            .computers
            .lock()
            .unwrap()
            .get(id)
            .map(|c| c.asleep)
    }
    pub fn network(&self, id: &str) -> Value {
        self.shared
            .computers
            .lock()
            .unwrap()
            .get(id)
            .map(|c| c.network.clone())
            .unwrap_or(Value::Null)
    }
    /// The provider loses this computer, as if it was removed on its side.
    pub fn lose(&self, id: &str) {
        self.shared.computers.lock().unwrap().remove(id);
    }
    pub fn exists(&self, id: &str) -> bool {
        self.shared.computers.lock().unwrap().contains_key(id)
    }
    /// A providers file using this stand-in for every size.
    pub fn providers_file(&self, dir: &std::path::Path) -> PathBuf {
        let key = dir.join("e2b.key");
        std::fs::write(&key, "fake-key\n").unwrap();
        let file = dir.join("computers.json");
        let route = |image: &str| json!([{"provider": "e2b", "image": image, "usd_per_hour": 0.1}]);
        std::fs::write(&file, json!({
            "sizes": [
                {"size": "small", "cpu": 1, "memory_gb": 2, "disk_gb": 10, "usd_per_hour": 0.09, "routes": route("cuckoo-small")},
                {"size": "standard", "cpu": 2, "memory_gb": 4, "disk_gb": 20, "usd_per_hour": 0.18, "routes": route("cuckoo-standard")}],
            "providers": [{"provider": "e2b", "key_file": key, "base_url": self.url, "computer_url": self.url}],
        }).to_string()).unwrap();
        file
    }
}

pub(crate) async fn start() -> Fake {
    let root = tempfile::tempdir().unwrap();
    let shared = Shared {
        root: root.path().to_owned(),
        ..Default::default()
    };
    let app = Router::new()
        .route("/v2/sandboxes", post(create))
        .route("/sandboxes/{id}/connect", post(wake))
        .route("/sandboxes/{id}/pause", post(pause))
        .route("/sandboxes/{id}", delete(remove))
        .route("/sandboxes/{id}/network", axum::routing::put(network))
        .route("/process.Process/Start", post(run))
        .route("/process.Process/SendInput", post(send_input))
        .route("/files", post(write).get(read))
        .with_state(shared.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    Fake {
        url,
        _root: root,
        shared,
    }
}

fn key_ok(headers: &HeaderMap) -> bool {
    headers.get("x-api-key").is_some_and(|k| k == "fake-key")
}
fn note(s: &Shared, line: String) {
    s.log.lock().unwrap().push(line);
}

async fn create(State(s): State<Shared>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    if !key_ok(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let id = {
        let mut next = s.next.lock().unwrap();
        *next += 1;
        format!("fake{next}")
    };
    let home = s.root.join(&id).join("home");
    std::fs::create_dir_all(&home).unwrap();
    note(
        &s,
        format!("create {}", body["templateID"].as_str().unwrap_or("")),
    );
    let token = format!("token-{id}");
    s.computers.lock().unwrap().insert(
        id.clone(),
        Computer {
            home,
            token: token.clone(),
            asleep: false,
            network: body["network"].clone(),
        },
    );
    (
        StatusCode::CREATED,
        Json(json!({"sandboxID": id, "envdAccessToken": token})),
    )
        .into_response()
}

async fn wake(State(s): State<Shared>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    if !key_ok(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut computers = s.computers.lock().unwrap();
    let Some(computer) = computers.get_mut(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    computer.asleep = false;
    let token = computer.token.clone();
    drop(computers);
    note(&s, format!("wake {id}"));
    (
        StatusCode::CREATED,
        Json(json!({"sandboxID": id, "envdAccessToken": token})),
    )
        .into_response()
}

async fn pause(State(s): State<Shared>, Path(id): Path<String>) -> StatusCode {
    let mut computers = s.computers.lock().unwrap();
    let Some(computer) = computers.get_mut(&id) else {
        return StatusCode::NOT_FOUND;
    };
    if computer.asleep {
        return StatusCode::CONFLICT;
    }
    computer.asleep = true;
    drop(computers);
    note(&s, format!("pause {id}"));
    StatusCode::NO_CONTENT
}

async fn remove(State(s): State<Shared>, Path(id): Path<String>) -> StatusCode {
    if s.computers.lock().unwrap().remove(&id).is_none() {
        return StatusCode::NOT_FOUND;
    }
    note(&s, format!("delete {id}"));
    StatusCode::NO_CONTENT
}

async fn network(
    State(s): State<Shared>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> StatusCode {
    let mut computers = s.computers.lock().unwrap();
    let Some(computer) = computers.get_mut(&id) else {
        return StatusCode::NOT_FOUND;
    };
    computer.network = body;
    StatusCode::NO_CONTENT
}

/// The awake computer a request is for, checked like E2B checks it.
fn computer(s: &Shared, headers: &HeaderMap) -> Result<PathBuf, StatusCode> {
    let id = headers
        .get("e2b-sandbox-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let computers = s.computers.lock().unwrap();
    let computer = computers.get(id).ok_or(StatusCode::NOT_FOUND)?;
    let token = headers
        .get("x-access-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if token != computer.token {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if computer.asleep {
        return Err(StatusCode::BAD_GATEWAY);
    }
    Ok(computer.home.clone())
}

fn inside(home: &std::path::Path, path: &str) -> PathBuf {
    home.join(
        path.strip_prefix(super::HOME)
            .unwrap_or(path)
            .trim_start_matches('/'),
    )
}

fn frame(flags: u8, message: &Value) -> Vec<u8> {
    let data = serde_json::to_vec(message).unwrap();
    let mut out = vec![flags];
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&data);
    out
}

async fn run(State(s): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let home = match computer(&s, &headers) {
        Ok(home) => home,
        Err(status) => return status.into_response(),
    };
    let message: Value = serde_json::from_slice(&body[5..]).unwrap_or(Value::Null);
    let process = &message["process"];
    let args: Vec<String> = process["args"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a.as_str().map(str::to_owned))
        .collect();
    // Like E2B: the working folder must exist. Commands refer to the computer's own
    // paths, so they are mapped into this computer's folder.
    let cwd = inside(&home, process["cwd"].as_str().unwrap_or(super::HOME));
    if !cwd.is_dir() {
        let error =
            json!({"error": {"message": format!("cwd '{}' does not exist", cwd.display())}});
        return (
            [("content-type", "application/connect+json")],
            Body::from(frame(2, &error)),
        )
            .into_response();
    }
    let args: Vec<String> = args
        .into_iter()
        .map(|a| a.replace(super::HOME, &home.to_string_lossy()))
        .collect();
    let mut command = tokio::process::Command::new(process["cmd"].as_str().unwrap_or("/bin/false"));
    command.args(&args).current_dir(&cwd).env("HOME", &home);
    // The browser helper: a local Chromium for tests, on a port of this computer's own.
    if let Ok(chromium) = std::env::var("AYATI_TEST_CHROMIUM") {
        command.env("CUCKOO_CHROMIUM", chromium);
    }
    command.env("CUCKOO_BROWSER_PORT", browser_port(&home).to_string());
    if let (Some(tag), true) = (message["tag"].as_str(), message["stdin"] == true) {
        let id = headers
            .get("e2b-sandbox-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        return streamed(&s, command, format!("{id}/{tag}")).await;
    }
    let output = command.output().await;
    use base64::Engine as _;
    let encode = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    let mut out = frame(0, &json!({"event": {"start": {"pid": 1}}}));
    match output {
        Ok(done) => {
            if !done.stdout.is_empty() {
                out.extend(frame(
                    0,
                    &json!({"event": {"data": {"stdout": encode(&done.stdout)}}}),
                ));
            }
            if !done.stderr.is_empty() {
                out.extend(frame(
                    0,
                    &json!({"event": {"data": {"stderr": encode(&done.stderr)}}}),
                ));
            }
            out.extend(frame(0, &json!({"event": {"end": {"exitCode": done.status.code().unwrap_or(-1), "exited": true}}})));
            out.extend(frame(2, &json!({})));
        }
        Err(error) => out.extend(frame(2, &json!({"error": {"message": error.to_string()}}))),
    }
    (
        [("content-type", "application/connect+json")],
        Body::from(out),
    )
        .into_response()
}

/// A port for this computer's browser, so tests running at once don't share one.
fn browser_port(home: &std::path::Path) -> u16 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    home.hash(&mut hasher);
    20_000 + (hasher.finish() % 20_000) as u16
}

/// A program whose input stays open: its output is sent as it comes, like E2B does.
async fn streamed(s: &Shared, mut command: tokio::process::Command, key: String) -> Response {
    use tokio::io::AsyncReadExt as _;
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    s.inputs
        .lock()
        .await
        .insert(key.clone(), child.stdin.take().unwrap());
    let mut stdout = child.stdout.take().unwrap();
    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(64);
    let inputs = s.inputs.clone();
    tokio::spawn(async move {
        use base64::Engine as _;
        let encode = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
        let _ = sender
            .send(Ok(frame(0, &json!({"event": {"start": {"pid": 1}}}))))
            .await;
        let mut chunk = vec![0u8; 65536];
        while let Ok(n) = stdout.read(&mut chunk).await {
            if n == 0 {
                break;
            }
            let data = json!({"event": {"data": {"stdout": encode(&chunk[..n])}}});
            if sender.send(Ok(frame(0, &data))).await.is_err() {
                break;
            }
        }
        let code = child.wait().await.ok().and_then(|s| s.code()).unwrap_or(-1);
        inputs.lock().await.remove(&key);
        let _ = sender
            .send(Ok(frame(
                0,
                &json!({"event": {"end": {"exitCode": code, "exited": true}}}),
            )))
            .await;
        let _ = sender.send(Ok(frame(2, &json!({})))).await;
    });
    (
        [("content-type", "application/connect+json")],
        Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(receiver)),
    )
        .into_response()
}

async fn send_input(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> StatusCode {
    use base64::Engine as _;
    use tokio::io::AsyncWriteExt as _;
    if computer(&s, &headers).is_err() {
        return StatusCode::UNAUTHORIZED;
    }
    let id = headers
        .get("e2b-sandbox-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let key = format!("{id}/{}", body["process"]["tag"].as_str().unwrap_or(""));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(body["input"]["stdin"].as_str().unwrap_or(""))
        .unwrap_or_default();
    let mut inputs = s.inputs.lock().await;
    let Some(input) = inputs.get_mut(&key) else {
        return StatusCode::NOT_FOUND;
    };
    if input.write_all(&bytes).await.is_err() || input.flush().await.is_err() {
        return StatusCode::GONE;
    }
    StatusCode::OK
}

async fn write(
    State(s): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let home = match computer(&s, &headers) {
        Ok(home) => home,
        Err(status) => return status.into_response(),
    };
    let path = inside(&home, q.get("path").map(String::as_str).unwrap_or(""));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&path, &body).unwrap();
    Json(json!([{"path": q.get("path"), "type": "file"}])).into_response()
}

async fn read(
    State(s): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let home = match computer(&s, &headers) {
        Ok(home) => home,
        Err(status) => return status.into_response(),
    };
    match std::fs::read(inside(
        &home,
        q.get("path").map(String::as_str).unwrap_or(""),
    )) {
        Ok(bytes) => bytes.into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
