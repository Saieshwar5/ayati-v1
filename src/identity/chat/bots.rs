//! Bots: a Slack-compatible incoming webhook per bot and channel, and the
//! routes that add, list and remove bots (`postgres/chat_bots.rs`).
//!
//! `POST /hooks/chat/{bot}/{secret}` takes Slack's JSON (`text`, `blocks`,
//! `attachments`) and posts it as the bot. Slack's markup becomes Cuckoo
//! Markdown: `<url|label>` a link, headers bold, fields and context lines,
//! buttons with a `url` links (other buttons need Slack's interactivity and
//! are left out). Anything that could act inside Cuckoo is neutralised: an
//! `@mention`, a run link or a workspace path in the text stays plain text,
//! so a bot can never mention, alert or open things as a person would.
use super::*;
use crate::identity::postgres::chat_bots::Posted;
use axum::extract::DefaultBodyLimit;

const MAX_CHARS: usize = 12_000;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/hooks/chat/{bot}/{secret}",
            post(hook).layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/bots",
            get(list).post(create),
        )
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/bots/{bot}/remove",
            post(remove),
        )
}

/// The hook takes no session or Origin: its secret is the credential.
pub(crate) fn hook_path(path: &str) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    parts.len() == 5
        && parts[0].is_empty()
        && parts[1..3] == ["hooks", "chat"]
        && uuid::Uuid::parse_str(parts[3]).is_ok()
        && !parts[4].is_empty()
}

async fn hook(
    State(app): State<Shared>,
    RoutePath((bot, secret)): RoutePath<(String, String)>,
    body: axum::body::Bytes,
) -> Response {
    let reply = |status: StatusCode, text: &'static str| (status, text).into_response();
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return reply(StatusCode::BAD_REQUEST, "invalid_payload");
    };
    let Some(content) = markdown(&payload) else {
        return reply(StatusCode::BAD_REQUEST, "no_text");
    };
    match app.store.bot_post(&bot, &secret, &content).await {
        Ok(Posted::Sent(_)) => reply(StatusCode::OK, "ok"),
        Ok(Posted::Unknown) => reply(StatusCode::NOT_FOUND, "no_service"),
        Ok(Posted::Archived) => reply(StatusCode::GONE, "channel_is_archived"),
        Ok(Posted::Limited) => reply(StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
        Err(_) => reply(StatusCode::SERVICE_UNAVAILABLE, "try_again"),
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NewBot {
    name: String,
}

async fn create(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<NewBot>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let (bot, secret) = app.store.create_bot(&user, &w, &chat, &input.name).await?;
    let url = format!(
        "{}/hooks/chat/{}/{secret}",
        app.config.origin,
        bot["id"].as_str().unwrap_or_default()
    );
    // The address is shown once: only its hash is kept.
    Ok(Json(json!({ "bot": bot, "url": url })))
}

async fn list(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = super::super::identity_for_read(&app, &headers).await?;
    Ok(Json(app.store.chat_bots(&user, &w, &chat).await?))
}

async fn remove(
    State(app): State<Shared>,
    RoutePath((w, chat, bot)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.remove_bot(&user, &w, &chat, &bot).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Slack's payload as Cuckoo Markdown, or None when it says nothing.
pub(crate) fn markdown(payload: &Value) -> Option<String> {
    let mut lines: Vec<String> = vec![];
    let blocks = payload["blocks"].as_array().filter(|b| !b.is_empty());
    match blocks {
        Some(blocks) => blocks
            .iter()
            .for_each(|block| block_lines(block, &mut lines)),
        None => {
            if let Some(text) = payload["text"].as_str() {
                lines.push(mrkdwn(text));
            }
        }
    }
    for attachment in payload["attachments"].as_array().into_iter().flatten() {
        attachment_lines(attachment, &mut lines);
    }
    let text = lines
        .into_iter()
        .map(|l| l.trim_end().to_owned())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let text = neutral(&text);
    (!text.trim().is_empty()).then(|| text.chars().take(MAX_CHARS).collect())
}

fn text_of(value: &Value) -> Option<String> {
    value["text"]
        .as_str()
        .or_else(|| value.as_str())
        .map(mrkdwn)
}

fn block_lines(block: &Value, lines: &mut Vec<String>) {
    match block["type"].as_str().unwrap_or_default() {
        "header" => {
            lines.extend(text_of(&block["text"]).map(|t| format!("**{}**", t.replace('*', ""))))
        }
        "section" => {
            lines.extend(text_of(&block["text"]));
            let fields: Vec<String> = block["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(text_of)
                .collect();
            if !fields.is_empty() {
                lines.push(fields.join("\n"));
            }
            lines.extend(button(&block["accessory"]));
        }
        "context" => {
            let parts: Vec<String> = block["elements"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(text_of)
                .collect();
            if !parts.is_empty() {
                lines.push(format!("_{}_", parts.join(" · ")));
            }
        }
        "divider" => lines.push("---".into()),
        "actions" => {
            let buttons: Vec<String> = block["elements"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(button)
                .collect();
            if !buttons.is_empty() {
                lines.push(buttons.join(" · "));
            }
        }
        _ => {}
    }
}

/// A button with a web address is a link; others need Slack interactivity.
fn button(element: &Value) -> Option<String> {
    let url = element["url"].as_str().filter(|u| web(u))?;
    let label = element["text"]["text"]
        .as_str()
        .unwrap_or("Open")
        .replace(['[', ']'], "");
    Some(format!("[{label}]({url})"))
}

fn attachment_lines(attachment: &Value, lines: &mut Vec<String>) {
    if let Some(title) = attachment["title"].as_str() {
        let title = mrkdwn(title).replace(['[', ']'], "");
        lines.push(match attachment["title_link"].as_str().filter(|u| web(u)) {
            Some(link) => format!("**[{title}]({link})**"),
            None => format!("**{title}**"),
        });
    }
    if let Some(text) = attachment["text"]
        .as_str()
        .or_else(|| attachment["fallback"].as_str())
    {
        lines.push(mrkdwn(text));
    }
    let fields: Vec<String> = attachment["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| {
            Some(format!(
                "**{}**: {}",
                f["title"].as_str()?.replace('*', ""),
                mrkdwn(f["value"].as_str().unwrap_or_default())
            ))
        })
        .collect();
    if !fields.is_empty() {
        lines.push(fields.join("\n"));
    }
}

fn web(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !url.contains(char::is_whitespace)
        && !url.contains([')', '<', '>'])
}

/// Slack markup: `<url|label>` and `<url>` links, `<@U1>` and `<!here>` plain words.
fn mrkdwn(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let inner = &rest[open + 1..open + close];
        let (target, label) = inner.split_once('|').unwrap_or((inner, ""));
        if web(target) || target.starts_with("mailto:") {
            let label = if label.is_empty() { target } else { label };
            out.push_str(&format!("[{}]({target})", label.replace(['[', ']'], "")));
        } else if let Some(special) = target.strip_prefix('!') {
            out.push('@');
            out.push_str(special.split('^').next().unwrap_or(special));
        } else if target.starts_with('@') || target.starts_with('#') {
            out.push_str(if label.is_empty() { target } else { label });
        } else {
            out.push_str(&rest[open..open + close + 1]);
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Link targets that mean something inside Cuckoo (mentions, runs, workspace
/// paths) lose their target, so they render as text.
fn neutral(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        out.push_str(&rest[..at + 2]);
        rest = &rest[at + 2..];
        let lower = rest.to_ascii_lowercase();
        if !(lower.starts_with("https://")
            || lower.starts_with("http://")
            || lower.starts_with("mailto:"))
        {
            out.push('#');
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[path = "bots_tests.rs"]
mod tests;
