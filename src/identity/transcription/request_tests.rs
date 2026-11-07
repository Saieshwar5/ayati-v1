//! The transcription request against a loopback provider that reads the form
//! the way an OpenAI-compatible API does.
use super::request::{Failure, Heard, Settings, parse, transcribe};
use axum::{
    Json, Router,
    extract::{Multipart, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the fixture provider received, and how it answers next.
#[derive(Default)]
pub(super) struct Provider {
    pub seen: Vec<Value>,
    pub answer: Option<(u16, String)>,
    pub slow: bool,
}
pub(super) type Shared = Arc<Mutex<Provider>>;

async fn transcriptions(
    State(provider): State<Shared>,
    headers: HeaderMap,
    mut form: Multipart,
) -> Response {
    let mut fields = serde_json::Map::new();
    fields.insert(
        "authorization".into(),
        json!(headers["authorization"].to_str().unwrap()),
    );
    while let Some(field) = form.next_field().await.unwrap() {
        let name = field.name().unwrap().to_owned();
        let file_name = field.file_name().map(str::to_owned);
        let mime = field.content_type().map(str::to_owned);
        let bytes = field.bytes().await.unwrap();
        fields.insert(
            name,
            json!({"file_name": file_name, "mime": mime, "value": String::from_utf8_lossy(&bytes)}),
        );
    }
    let (answer, slow) = {
        let mut provider = provider.lock().unwrap();
        provider.seen.push(Value::Object(fields));
        (provider.answer.clone(), provider.slow)
    };
    if slow {
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    match answer {
        Some((status, body)) => (StatusCode::from_u16(status).unwrap(), body).into_response(),
        None => Json(json!({"text": " Hello from the voice note. "})).into_response(),
    }
}

/// A loopback provider and a providers file naming it, with its key file.
pub(super) async fn provider() -> (Shared, tempfile::TempDir, tokio::task::JoinHandle<()>) {
    let state = Shared::default();
    let router = Router::new()
        .route("/v1/audio/transcriptions", post(transcriptions))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async { axum::serve(listener, router).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let key = dir.path().join("audio-key");
    std::fs::write(&key, "audio-secret-key\n").unwrap();
    std::fs::write(
        dir.path().join("providers.json"),
        json!([{"provider":"fixture-audio","base_url":base,"key_file":key}]).to_string(),
    )
    .unwrap();
    (state, dir, server)
}

fn settings<'a>(file: &'a std::path::Path, provider: &'a str) -> Settings<'a> {
    Settings {
        file,
        provider,
        model: "whisper-v3-turbo",
        time_limit: Duration::from_millis(1500),
    }
}

#[tokio::test]
async fn sends_the_audio_as_a_form_and_reads_the_text() {
    let (state, dir, server) = provider().await;
    let file = dir.path().join("providers.json");
    let heard = transcribe(
        &settings(&file, "fixture-audio"),
        "voice-message.webm",
        "audio/webm",
        b"\x1a\x45\xdf\xa3 recorded audio".to_vec(),
    )
    .await
    .unwrap();
    assert_eq!(
        heard,
        Heard {
            text: " Hello from the voice note. ".into(),
            duration_ms: None
        }
    );
    let seen = state.lock().unwrap().seen.remove(0);
    assert_eq!(seen["authorization"], "Bearer audio-secret-key");
    assert_eq!(seen["file"]["file_name"], "voice-message.webm");
    assert_eq!(seen["file"]["mime"], "audio/webm");
    assert!(
        seen["file"]["value"]
            .as_str()
            .unwrap()
            .ends_with("recorded audio")
    );
    assert_eq!(seen["model"]["value"], "whisper-v3-turbo");
    assert_eq!(seen["model"]["file_name"], Value::Null);
    assert_eq!(seen["response_format"]["value"], "json");
    server.abort();
}

#[tokio::test]
async fn provider_refusals_say_whether_to_try_again() {
    let (state, dir, server) = provider().await;
    let file = dir.path().join("providers.json");
    let settings = settings(&file, "fixture-audio");
    let attempt = |status: u16, body: &str| {
        state.lock().unwrap().answer = Some((status, body.into()));
        transcribe(&settings, "voice.ogg", "audio/ogg", b"OggS".to_vec())
    };
    let busy = attempt(503, r#"{"error":"busy"}"#).await;
    assert_eq!(
        busy,
        Err(Failure {
            status: 503,
            retry: true
        })
    );
    let limited = attempt(429, "{}").await;
    assert_eq!(
        limited,
        Err(Failure {
            status: 429,
            retry: true
        })
    );
    let refused = attempt(400, r#"{"error":"unsupported audio"}"#).await;
    assert_eq!(
        refused,
        Err(Failure {
            status: 400,
            retry: false
        })
    );
    let unreadable = attempt(200, "not json").await;
    assert_eq!(
        unreadable,
        Err(Failure {
            status: 200,
            retry: false
        })
    );
    let timed = attempt(200, r#"{"text":"Ready","duration":4.2}"#).await;
    assert_eq!(timed.unwrap().duration_ms, Some(4200));

    // Too slow: the time limit ends the request and it may be tried again.
    state.lock().unwrap().slow = true;
    let started = std::time::Instant::now();
    let slow = transcribe(&settings, "voice.ogg", "audio/ogg", b"OggS".to_vec()).await;
    assert_eq!(
        slow,
        Err(Failure {
            status: 0,
            retry: true
        })
    );
    assert!(started.elapsed() < Duration::from_secs(4));
    server.abort();

    // A provider missing from the providers file is never called.
    let missing = transcribe(
        &super::request::Settings {
            provider: "not-configured",
            ..settings
        },
        "voice.ogg",
        "audio/ogg",
        vec![1],
    )
    .await;
    assert_eq!(
        missing,
        Err(Failure {
            status: 0,
            retry: true
        })
    );
}

#[test]
fn answers_must_carry_text() {
    assert_eq!(
        parse(br#"{"text":"","duration":0}"#),
        Some(Heard {
            text: String::new(),
            duration_ms: Some(0)
        })
    );
    assert_eq!(parse(br#"{"duration":3}"#), None);
    assert_eq!(parse(br#"{"text":3}"#), None);
    assert_eq!(
        parse(br#"{"text":"ok","duration":-1}"#)
            .unwrap()
            .duration_ms,
        None
    );
}
