//! Reads Postmark's inbound webhook into what Cuckoo keeps: who wrote, to which
//! address, whether the sender's domain vouched for it, the thread it belongs
//! to and the new text. Nothing here trusts a header Postmark did not add.
use super::settings::normal_address;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;

/// The model reads at most this much of one email.
const TEXT_BYTES: usize = 11_900;
const CUT: &str = "\n\n[The rest of this email was cut.]";
/// Message IDs kept in a reply's References header.
const REFERENCES: usize = 20;
const SUBJECT_CHARS: usize = 200;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Payload {
    #[serde(rename = "MessageID")]
    message_id: String,
    from_full: Mailbox,
    #[serde(default)]
    to_full: Vec<Mailbox>,
    #[serde(default)]
    cc_full: Vec<Mailbox>,
    #[serde(default)]
    original_recipient: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    text_body: String,
    #[serde(default)]
    stripped_text_reply: String,
    #[serde(default)]
    headers: Vec<Header>,
    #[serde(default)]
    attachments: Vec<serde_json::Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Mailbox {
    email: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Header {
    name: String,
    value: String,
}

/// One email, as Cuckoo understands it.
#[derive(Debug, PartialEq)]
pub(crate) struct Parsed {
    /// Postmark's ID for this delivery; a retry carries the same one.
    pub event: String,
    pub from: String,
    /// The Cuckoo address it was delivered to.
    pub to: String,
    /// Someone besides the agent was on To or Cc.
    pub others: bool,
    /// The sender's own domain vouched for it (DKIM or SPF, aligned with From).
    pub authenticated: bool,
    /// An auto-reply, bounce or list mail: never answered.
    pub automated: bool,
    /// This email's Message-ID, with its angle brackets.
    pub message_id: String,
    /// The first message of its thread: one room per thread.
    pub root: String,
    /// The References a reply carries, oldest first, ending with `message_id`.
    pub references: Vec<String>,
    pub subject: String,
    /// The new text, without quoted history. `None` when there is none to read.
    pub text: Option<String>,
    pub attachments: bool,
}

pub(crate) fn parse(body: &[u8]) -> Result<Parsed> {
    let p: Payload = serde_json::from_slice(body).context("Unreadable inbound email")?;
    crate::identity::identifier(&p.message_id)?;
    let from = normal_address(&p.from_full.email).context("Unreadable sender address")?;
    let to = normal_address(&p.original_recipient)
        .or_else(|| p.to_full.first().and_then(|m| normal_address(&m.email)))
        .context("Unreadable recipient address")?;
    let others = p
        .to_full
        .iter()
        .chain(&p.cc_full)
        .any(|m| normal_address(&m.email).as_deref() != Some(to.as_str()));
    let first = |name: &str| first_header(&p.headers, name);
    let message_id = ids(first("Message-ID").unwrap_or_default())
        .into_iter()
        .next()
        .unwrap_or_else(|| format!("<{}@postmark.invalid>", p.message_id));
    let mut references = ids(first("References").unwrap_or_default());
    let parent = ids(first("In-Reply-To").unwrap_or_default())
        .into_iter()
        .next();
    if let Some(parent) = &parent
        && !references.contains(parent)
    {
        references.push(parent.clone());
    }
    let root = references
        .first()
        .cloned()
        .unwrap_or_else(|| message_id.clone());
    references.retain(|r| *r != message_id);
    references.push(message_id.clone());
    let skip = references.len().saturating_sub(REFERENCES);
    references.drain(..skip);
    let text = if p.stripped_text_reply.trim().is_empty() {
        new_text(&p.text_body)
    } else {
        p.stripped_text_reply.trim().to_owned()
    };
    Ok(Parsed {
        event: p.message_id.clone(),
        authenticated: authenticated(&from, first("Received-SPF"), first("X-Spam-Tests")),
        automated: automated(&from, &p.headers),
        others,
        from,
        to,
        message_id,
        root,
        references,
        subject: subject(&p.subject),
        text: (!text.is_empty()).then(|| bounded(text)),
        attachments: !p.attachments.is_empty(),
    })
}

/// `<id>` tokens of a Message-ID, In-Reply-To or References header.
fn ids(value: &str) -> Vec<String> {
    let mut found = vec![];
    for part in value.split('<').skip(1) {
        let Some((id, _)) = part.split_once('>') else {
            continue;
        };
        let ok = !id.is_empty()
            && id.len() <= 250
            && !id
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '<');
        let id = format!("<{id}>");
        if ok && !found.contains(&id) {
            found.push(id);
        }
    }
    found
}

/// Only the topmost copy of each header counts: Postmark's own servers add it
/// above everything the sender wrote. Live mail must confirm that order.
fn authenticated(from: &str, spf: Option<&str>, spam_tests: Option<&str>) -> bool {
    let domain = from.rsplit_once('@').map_or("", |(_, d)| d);
    let dkim = spam_tests.is_some_and(|tests| {
        tests
            .split(|c: char| c == ',' || c.is_whitespace())
            .any(|t| t.eq_ignore_ascii_case("DKIM_VALID_AU"))
    });
    let spf = spf.is_some_and(|value| {
        let value = value.trim().to_ascii_lowercase();
        value.starts_with("pass")
            && value
                .split("envelope-from=")
                .nth(1)
                .and_then(|rest| rest.split([';', ' ']).next())
                .and_then(|sender| sender.trim_matches(['<', '>', '"']).rsplit_once('@'))
                .is_some_and(|(_, d)| d == domain)
    });
    dkim || spf
}

fn first_header<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

fn automated(from: &str, headers: &[Header]) -> bool {
    let has = |name: &str| first_header(headers, name).is_some();
    let value = |name: &str| {
        first_header(headers, name)
            .map(|v| v.trim().to_ascii_lowercase())
            .unwrap_or_default()
    };
    let local = from.split('@').next().unwrap_or_default();
    (has("Auto-Submitted") && value("Auto-Submitted") != "no")
        || ["bulk", "junk", "list", "auto_reply"].contains(&value("Precedence").as_str())
        || has("List-Id")
        || has("List-Unsubscribe")
        || has("X-Autoreply")
        || has("X-Autorespond")
        || value("Return-Path") == "<>"
        || ["mailer-daemon", "postmaster"].contains(&local)
        || local.starts_with("noreply")
        || local.starts_with("no-reply")
}

/// The text above the quoted history, for mail Postmark could not split.
fn new_text(body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut kept = vec![];
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let next = lines.get(i + 1).map_or("", |l| l.trim());
        let quote_header =
            trimmed.starts_with("On ") && (trimmed.ends_with("wrote:") || next.ends_with("wrote:"));
        if quote_header
            || trimmed == "-----Original Message-----"
            || (trimmed.len() >= 30 && trimmed.chars().all(|c| c == '_'))
        {
            break;
        }
        if !trimmed.starts_with('>') {
            kept.push(*line);
        }
    }
    kept.join("\n").trim().to_owned()
}

fn bounded(text: String) -> String {
    if text.len() <= TEXT_BYTES {
        return text;
    }
    let mut end = TEXT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{CUT}", &text[..end])
}

fn subject(value: &str) -> String {
    let clean: String = value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(SUBJECT_CHARS)
        .collect();
    clean.trim().to_owned()
}

/// The subject of a reply: `Re:` once, never stacked.
pub(crate) fn reply_subject(original: &str) -> String {
    let lower = original.to_ascii_lowercase();
    if original.is_empty() {
        "Re: your message".into()
    } else if lower.starts_with("re:") {
        original.into()
    } else {
        format!("Re: {original}")
    }
}

/// Checks the parts the shared intake relies on, with readable reasons.
pub(crate) fn admit(parsed: &Parsed) -> Result<&str> {
    let text = parsed.text.as_deref().context("No plain text to read")?;
    ensure!(parsed.root.len() <= 320, "Thread identifier too long");
    Ok(text)
}

#[cfg(test)]
#[path = "inbound_tests.rs"]
mod tests;
