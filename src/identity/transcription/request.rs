//! One speech-to-text request: OpenAI's `POST {base_url}/audio/transcriptions`,
//! which Fireworks, OpenAI and other providers accept. The key comes from the
//! provider's key file; neither the key, the audio nor the text is logged.
use reqwest::multipart::{Form, Part};
use serde_json::Value;
use std::time::Duration;

pub(crate) struct Settings<'a> {
    pub file: &'a std::path::Path,
    pub provider: &'a str,
    pub model: &'a str,
    /// The whole request, uploading the audio included.
    pub time_limit: Duration,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Heard {
    pub text: String,
    /// When the provider says how long the audio was.
    pub duration_ms: Option<i64>,
}

/// Why a request didn't return a transcript. `status` is the provider's HTTP
/// status, or 0 when it couldn't be reached or answered too late.
#[derive(Debug, PartialEq)]
pub(crate) struct Failure {
    pub status: u16,
    pub retry: bool,
}
impl Failure {
    /// Unreachable, cut off or too slow: worth another try.
    const UNREACHABLE: Self = Self {
        status: 0,
        retry: true,
    };
    /// Busy (408, 429, 5xx) is worth another try; other refusals aren't.
    fn refused(status: reqwest::StatusCode) -> Self {
        Self {
            status: status.as_u16(),
            retry: status == reqwest::StatusCode::REQUEST_TIMEOUT
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || status.is_server_error(),
        }
    }
}

/// Larger answers aren't a transcript of five minutes of speech.
const MAX_ANSWER: usize = 1024 * 1024;

pub(crate) async fn transcribe(
    settings: &Settings<'_>,
    name: &str,
    mime: &str,
    audio: Vec<u8>,
) -> Result<Heard, Failure> {
    use crate::identity::worker_control::{model_client, model_provider};
    // A key file being rotated may be briefly unreadable; tries are bounded.
    let (base, key) =
        model_provider(settings.file, settings.provider).map_err(|_| Failure::UNREACHABLE)?;
    let form = form(settings.model, name, mime, audio).map_err(|_| Failure {
        status: 0,
        retry: false,
    })?;
    let request = async {
        let mut response = model_client()
            .post(format!("{base}/audio/transcriptions"))
            .bearer_auth(key)
            .multipart(form)
            .send()
            .await
            .map_err(|_| Failure::UNREACHABLE)?;
        let status = response.status();
        if !status.is_success() {
            return Err(Failure::refused(status));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Failure::UNREACHABLE)? {
            body.extend_from_slice(&chunk);
            if body.len() > MAX_ANSWER {
                return Err(Failure::refused(status));
            }
        }
        parse(&body).ok_or(Failure::refused(status))
    };
    tokio::time::timeout(settings.time_limit, request)
        .await
        .unwrap_or(Err(Failure::UNREACHABLE))
}

/// The form fields every OpenAI-compatible transcription API takes.
pub(super) fn form(model: &str, name: &str, mime: &str, audio: Vec<u8>) -> reqwest::Result<Form> {
    let file = Part::bytes(audio)
        .file_name(name.to_owned())
        .mime_str(mime)?;
    Ok(Form::new()
        .part("file", file)
        .text("model", model.to_owned())
        .text("response_format", "json"))
}

/// `{"text": ...}`, with `duration` in seconds when the provider includes it.
pub(super) fn parse(body: &[u8]) -> Option<Heard> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let text = value["text"].as_str()?.to_owned();
    let duration_ms = value["duration"]
        .as_f64()
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0 && *seconds < 86_400.0)
        .map(|seconds| (seconds * 1000.0).round() as i64);
    Some(Heard { text, duration_ms })
}
