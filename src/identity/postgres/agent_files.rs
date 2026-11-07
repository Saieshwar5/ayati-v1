//! What an agent knows of the files in its conversation evidence (schemas 95, 100).
//!
//! Each message lists its files by ID, name, type and size, with how far each
//! could be read; audio carries its transcript as before. The files of the
//! request, of the message it replies to and of the messages just before it
//! also bring their text, in a budget of their own outside the history budget,
//! so the agent answers from them at once. Older and longer files are read on
//! demand with the file tools (`conversation_files`). Everything here comes from
//! messages already selected for this task, so a file reaches an agent only
//! where the message that shared it does.
use super::super::*;
use serde_json::json;
use std::collections::HashMap;

#[cfg(test)]
#[path = "agent_files_tests.rs"]
mod tests;

/// Text from the request's files, across all of them.
const REQUEST_TEXT: usize = 24_000;
/// Text from any one file, so one long file can't crowd out the others.
const FILE_TEXT: usize = 12_000;
/// Less room than this left isn't worth a fragment; the file can be read instead.
const SMALLEST_PART: usize = 500;
/// Earlier messages whose files bring text, newest first, after the request
/// itself and the message it replies to.
const EARLIER_MESSAGES: usize = 10;

/// Replaces each message's attachment IDs with what an agent may know of them.
pub(in crate::identity::postgres) async fn describe(
    tx: &DatabaseTransaction,
    w: &str,
    chat: &str,
    messages: &mut [Value],
) -> Result<()> {
    let ids: Vec<String> = messages
        .iter()
        .flat_map(|m| m["attachments"].as_array().cloned().unwrap_or_default())
        .filter_map(|id| id.as_str().map(str::to_owned))
        .collect();
    let mut objects = HashMap::new();
    if !ids.is_empty() {
        for row in rows(tx, "SELECT id,name,mime,size FROM relay_objects WHERE workspace=$1 AND chat=$2 AND id IN (SELECT jsonb_array_elements_text($3::jsonb))", vec![w.into(), chat.into(), json!(ids).into()]).await? {
            let id: String = row.try_get("", "id")?;
            objects.insert(id, (row.try_get::<String>("", "name")?, row.try_get::<String>("", "mime")?, row.try_get::<i64>("", "size")?));
        }
    }
    let heard = super::super::transcripts::saved(tx, w, &ids).await?;
    let read = super::contents::saved(tx, w, &ids).await?;
    for message in messages.iter_mut() {
        let ids = message
            .as_object_mut()
            .and_then(|m| m.remove("attachments"))
            .and_then(|a| a.as_array().cloned())
            .unwrap_or_default();
        let files: Vec<Value> = ids
            .iter()
            .filter_map(|id| Some((id.as_str()?, objects.get(id.as_str()?)?)))
            .map(|(id, (name, mime, size))| {
                let mut file = json!({"file_id": id, "name": name, "mime": mime, "size": size});
                if mime.starts_with("audio/") {
                    describe_audio(&mut file, heard.get(id));
                } else {
                    describe_reading(&mut file, read.get(id));
                }
                file
            })
            .collect();
        if files.is_empty() {
            continue;
        }
        // A message that only shares files has no words of its own.
        if message["content"] == super::super::transcripts::FILES_ONLY {
            message["content"] = json!("");
        }
        message["attachments"] = json!(files);
    }
    Ok(())
}

fn describe_audio(file: &mut Value, saved: Option<&(String, Option<String>, Option<i32>)>) {
    file["transcript_status"] = json!(saved.map_or("not_enabled", |s| s.0.as_str()));
    if let Some(ms) = saved.and_then(|s| s.2) {
        file["duration_seconds"] = json!((i64::from(ms) + 500) / 1000);
    }
    if let Some(text) = saved.and_then(|s| s.1.as_deref()) {
        let kept: String = text
            .chars()
            .take(super::super::transcripts::AGENT_TEXT)
            .collect();
        if kept.len() < text.len() {
            file["transcript_truncated"] = json!(true);
        }
        file["transcript"] = json!(kept);
    }
}

/// How far a file could be read; its text comes separately (`request_files`).
fn describe_reading(file: &mut Value, reading: Option<&super::contents::Reading>) {
    file["reading"] = json!(super::contents::status(reading));
    let Some(reading) = reading else {
        return;
    };
    if let Some(kind) = &reading.kind {
        file["kind"] = json!(kind);
    }
    if let Some(text) = &reading.text {
        file["characters"] = json!(text.chars().count());
    }
    if reading.truncated {
        file["truncated"] = json!(true);
    }
}

/// The text of the files an agent needs first, from `messages` as `describe`
/// left them: the request's own files, then those of the message it replies
/// to, then those of the latest earlier messages, within one budget.
pub(in crate::identity::postgres) async fn request_files(
    tx: &DatabaseTransaction,
    w: &str,
    request: &str,
    messages: &[Value],
) -> Result<Vec<Value>> {
    let Some(asked) = messages.iter().position(|m| m["id"] == request) else {
        return Ok(vec![]);
    };
    let replied = messages[asked]["reply_to"].as_str();
    let mut order = vec![&messages[asked]];
    order.extend(messages.iter().filter(|m| replied == m["id"].as_str()));
    order.extend(
        messages[..asked]
            .iter()
            .rev()
            .filter(|m| replied != m["id"].as_str())
            .take(EARLIER_MESSAGES),
    );
    let wanted: Vec<(&Value, &Value)> = order
        .into_iter()
        .flat_map(|message| {
            message["attachments"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|file| file["reading"] == "ready")
                .map(move |file| (message, file))
        })
        .collect();
    let ids: Vec<String> = wanted
        .iter()
        .filter_map(|(_, file)| file["file_id"].as_str().map(str::to_owned))
        .collect();
    let read = super::contents::saved(tx, w, &ids).await?;
    let mut left = REQUEST_TEXT;
    let mut files = vec![];
    for (message, file) in wanted {
        let Some(text) = file["file_id"]
            .as_str()
            .and_then(|id| read.get(id))
            .and_then(|reading| reading.text.as_deref())
        else {
            continue;
        };
        if left < SMALLEST_PART {
            break;
        }
        let characters = text.chars().count();
        let shown: String = text.chars().take(FILE_TEXT.min(left)).collect();
        let count = shown.chars().count();
        left -= count;
        files.push(json!({
            "file_id": file["file_id"], "message": message["id"], "name": file["name"],
            "kind": file["kind"], "characters": characters, "text": shown,
            // The rest can be read from here with read_conversation_file.
            "next_offset": (count < characters).then_some(count),
        }));
    }
    Ok(files)
}
