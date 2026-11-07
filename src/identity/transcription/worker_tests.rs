//! A whole worker round: stored bytes, the provider, the saved transcript and
//! its usage row, against real PostgreSQL and a loopback provider.
use super::{Configured, request_tests::provider, round};
use crate::identity::{
    App, Provider,
    chat::model::{NewChat, NewMessage},
    collaboration::Object,
    postgres::tests::{fixture, user, workspace},
    workspaces::Runtimes,
};
use std::sync::Arc;

/// A WAV of silence: 8,000 bytes a second.
fn wav(seconds: u32) -> Vec<u8> {
    let data = 8_000 * seconds;
    let mut file = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0".to_vec();
    file.extend_from_slice(&8_000u32.to_le_bytes());
    file.extend_from_slice(&8_000u32.to_le_bytes());
    file.extend_from_slice(b"\x01\0\x08\0data");
    file.extend_from_slice(&data.to_le_bytes());
    file.resize(file.len() + data as usize, 0x80);
    file
}

fn checksum(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_round_transcribes_stored_audio_once_and_refuses_long_recordings() {
    let (state, dir, server) = provider().await;
    let mut config = crate::config::Controller::for_tests("http://127.0.0.1:8765");
    config.model_providers_file = Some(dir.path().join("providers.json"));
    config.transcription_model = Some(("fixture-audio".into(), "whisper-v3-turbo".into()));
    let configured = Configured::from(&config).unwrap();
    let app = Arc::new(App {
        library: Default::default(),
        provider: Provider::test_provider(),
        store: fixture().await,
        config: Arc::new(config),
        runtimes: Runtimes::new(dir.path().join("workspaces")),
        simulation: None,
    });
    let owner = user(&app.store, "transcript-worker-owner").await;
    let w = workspace(&app.store, &owner).await;
    let chat = uuid::Uuid::new_v4().to_string();
    app.store
        .chat_create(
            &owner,
            &w,
            &NewChat {
                id: chat.clone(),
                name: "Voice".into(),
                users: vec![],
                agents: vec![],
                kind: "group".into(),
                description: String::new(),
            },
        )
        .await
        .unwrap();
    let root = app.runtimes.object_root(&w);
    std::fs::create_dir_all(&root).unwrap();
    let mut uploads = vec![];
    for seconds in [3, 320] {
        let bytes = wav(seconds);
        let object = Object {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("voice-{seconds}.wav"),
            mime: "audio/wav".into(),
            size: bytes.len() as i64,
            checksum: checksum(&bytes),
        };
        std::fs::write(root.join(&object.id), &bytes).unwrap();
        app.store
            .save_object(&owner, &w, &chat, &object, true)
            .await
            .unwrap();
        let sent = NewMessage {
            reply_to: None,
            id: uuid::Uuid::new_v4().to_string(),
            content: "Shared files".into(),
            agent: None,
            agents: vec![],
            thread: None,
            attachments: vec![object.id.clone()],
        };
        app.store.chat_send(&owner, &w, &chat, &sent).await.unwrap();
        uploads.push(sent.id);
    }

    assert!(round(&app, &configured).await);
    assert!(!round(&app, &configured).await, "nothing left to take");
    // Only the three-second note reached the provider.
    let seen = std::mem::take(&mut state.lock().unwrap().seen);
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["file"]["file_name"], "voice-3.wav");
    let page = app.store.chat_page(&owner, &w, &chat, None).await.unwrap();
    let file = |id: &str| {
        page["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .unwrap()["attachments"][0]
            .clone()
    };
    let short = file(&uploads[0]);
    assert_eq!(short["transcript_status"], "ready");
    assert_eq!(short["transcript"], "Hello from the voice note.");
    assert_eq!(short["duration_ms"], 3000);
    let long = file(&uploads[1]);
    assert_eq!(long["transcript_status"], "failed");
    assert_eq!(long["duration_ms"], 320_000);
    // Operators see the call with the other model calls: count and speed.
    let operations = app.store.model_operations(0).await.unwrap();
    let model = operations["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["key"] == "whisper-v3-turbo")
        .cloned()
        .unwrap();
    assert_eq!(model["calls"], 1);
    assert_eq!(model["tokens"], 0);
    server.abort();
}
