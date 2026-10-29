//! Saved response presentation. This module grants no source or operation authority.
use anyhow::{Result, ensure};
use axum::{
    Json,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
pub(crate) const MAX_BODY: usize = 512 * 1024;
pub(crate) const PREVIEW_CHARS: usize = 900;
pub(crate) const EXCERPT_CHARS: usize = 320;

pub(crate) fn checksum(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub(crate) fn long(text: &str) -> bool {
    text.chars().take(PREVIEW_CHARS + 1).count() > PREVIEW_CHARS
        || text.bytes().filter(|b| *b == b'\n').take(9).count() > 8
}
fn excerpt(text: &str) -> String {
    text.chars().take(EXCERPT_CHARS).collect()
}
pub(crate) fn reference(id: &str, text: &str, state: &str, sequence: i64) -> Value {
    json!({"schema":1,"id":id,"version":checksum(text.as_bytes()),"sequence":sequence,
      "state":match state {"complete"|"sent"=>"complete","queued"|"streaming"=>"draft",_=>"interrupted"},
      "characters":text.chars().count(),"bytes":text.len(),"long":long(text),"available":text.len()<=MAX_BODY})
}
pub(crate) fn body(
    id: &str,
    text: &str,
    state: &str,
    sequence: i64,
    metadata: bool,
) -> Result<Value> {
    let mut result = reference(id, text, state, sequence);
    if !metadata {
        ensure!(
            text.len() <= MAX_BODY,
            "This saved response exceeds the supported reader size. Its stored content has not been removed."
        );
        result["body"] = json!(text);
        result["format"] = json!("markdown");
    }
    Ok(result)
}
pub(crate) fn compact_message(message: &mut Value) {
    if message["kind"] != "agent"
        || message["approval"].is_object()
        || message["response_unavailable"] == true
    {
        return;
    }
    let Some(id) = message["id"].as_str() else {
        return;
    };
    let text = message["progress"]["text"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| message["content"].as_str())
        .unwrap_or("");
    if text.is_empty() {
        return;
    }
    let data = reference(
        id,
        text,
        message["state"].as_str().unwrap_or("interrupted"),
        message["revision"].as_i64().unwrap_or(0),
    );
    if !message["response"].is_object() {
        message["response"] = data;
    }
    if let Some(text) = message["content"].as_str().filter(|text| long(text)) {
        message["content"] = json!(excerpt(text));
    }
    if let Some(text) = message["progress"]["text"]
        .as_str()
        .filter(|text| long(text))
    {
        message["progress"]["text"] = json!(excerpt(text));
    }
}
pub(crate) fn compact_history(page: &mut Value) {
    if let Some(messages) = page["messages"].as_array_mut() {
        for message in messages {
            compact_message(message);
        }
    }
}
pub(crate) fn compact_events(page: &mut Value) {
    if let Some(events) = page["events"].as_array_mut() {
        for event in events {
            if event["kind"] == "message" {
                compact_message(&mut event["message"]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previews_bound_unicode_streams_and_many_short_lines_without_changing_the_saved_body() {
        let text = "你好".repeat(700);
        let mut message = json!({"id":"reply","kind":"agent","state":"queued","revision":9,"content":"old checkpoint".repeat(300),"progress":{"state":"responding","text":text}});
        compact_message(&mut message);
        assert_eq!(
            message["progress"]["text"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            320
        );
        assert_eq!(message["content"].as_str().unwrap().chars().count(), 320);
        assert_eq!(message["response"]["version"], checksum(text.as_bytes()));
        assert_eq!(message["response"]["characters"], 1400);
        assert!(long(&"x\n".repeat(10)));
        assert!(!long("A short response"));
        let frozen = message.clone();
        compact_message(&mut message);
        assert_eq!(frozen, message);
        assert_eq!(
            body("reply", &text, "queued", 9, false).unwrap()["body"],
            text
        );
    }
    #[test]
    fn compact_modes_do_not_rewrite_human_messages_or_permission_controls() {
        let mut human = json!({"kind":"user","id":"human","content":"x".repeat(2000)});
        let original = human.clone();
        compact_message(&mut human);
        assert_eq!(human, original);
        let mut approval = json!({"kind":"agent","id":"approval","approval":{"id":"exact"},"content":"x".repeat(2000)});
        let original = approval.clone();
        compact_message(&mut approval);
        assert_eq!(approval, original);
        let mut denied = json!({"kind":"agent","id":"denied","response_unavailable":true,"content":"Response unavailable"});
        compact_message(&mut denied);
        assert!(denied.get("response").is_none());
    }
    #[test]
    fn reader_rejects_oversize_without_losing_content_and_labels_partial_work() {
        let text = "x".repeat(MAX_BODY + 1);
        assert!(body("large", &text, "complete", 1, false).is_err());
        let meta = body("large", &text, "complete", 1, true).unwrap();
        assert_eq!(meta["available"], false);
        assert!(meta.get("body").is_none());
        assert_eq!(
            body("partial", "Saved partial text", "failed", 2, false).unwrap()["state"],
            "interrupted"
        );
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Read {
    // Clients may still name the selected agent; reading never depends on it.
    #[serde(rename = "agent")]
    _agent: Option<String>,
    #[serde(default)]
    pub metadata: bool,
    #[serde(default)]
    pub download: bool,
    pub version: Option<String>,
}
pub(crate) fn deliver(id: &str, data: Value, input: &Read) -> Response {
    if input
        .version
        .as_ref()
        .is_some_and(|version| data["version"] != *version)
    {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error":"This saved version changed. Open the latest saved response."})),
        )
            .into_response();
    }
    let mut response = if input.download {
        if data["state"] != "complete" || data["body"].as_str().is_none() {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error":"Open a complete saved response before downloading."})),
            )
                .into_response();
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            "text/markdown; charset=utf-8".parse().unwrap(),
        );
        let name = crate::response::checksum(id.as_bytes());
        headers.insert(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"response-{}.md\"", &name[..12])
                .parse()
                .unwrap(),
        );
        (headers, data["body"].as_str().unwrap().to_owned()).into_response()
    } else {
        Json(data).into_response()
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
}
