//! One incoming mail, as Cuckoo needs it: who it says it is from, the thread it
//! continues, and the new text with the quoted history removed. The `From`
//! address is only a claim here; SES's DMARC verdict decides whether to believe
//! it (`notification`). Attachments are counted, never opened.
use mail_parser::{HeaderValue, MessageParser};

/// The most text kept from one mail, within the gateway's 12,000-byte limit.
const MAX_TEXT: usize = 10_000;
/// At most this many thread references are kept and sent back.
pub(crate) const MAX_REFERENCES: usize = 20;

#[derive(Debug, PartialEq)]
pub(crate) struct Mail {
    /// The mail's `Message-ID`, without angle brackets.
    pub message_id: Option<String>,
    /// The single `From` address, lowercased. `None` for none or several.
    pub from: Option<String>,
    pub subject: String,
    /// The new text: what the person wrote above the quoted history.
    pub text: String,
    /// `References` then `In-Reply-To`, oldest first, without brackets.
    pub parents: Vec<String>,
    /// A machine wrote it: an automatic reply, a bounce or a mailing list.
    pub automated: bool,
    pub attachments: usize,
}

pub(crate) fn parse(raw: &[u8]) -> Option<Mail> {
    let message = MessageParser::default().parse(raw)?;
    let from = match message.from().map(|a| a.iter().collect::<Vec<_>>()) {
        Some(list) if list.len() == 1 => list[0].address().map(str::to_lowercase),
        _ => None,
    };
    let mut parents = ids(message.references());
    for id in ids(message.in_reply_to()) {
        if !parents.contains(&id) {
            parents.push(id);
        }
    }
    if parents.len() > MAX_REFERENCES {
        parents.drain(..parents.len() - MAX_REFERENCES);
    }
    let body = message.body_text(0).unwrap_or_default();
    Some(Mail {
        message_id: message.message_id().map(str::to_owned),
        from,
        subject: message
            .subject()
            .unwrap_or_default()
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect(),
        text: bounded(&new_text(&body)),
        parents,
        automated: automated(&message),
        attachments: message.attachment_count(),
    })
}

fn ids(value: &HeaderValue) -> Vec<String> {
    let list: Vec<String> = match value {
        HeaderValue::Text(id) => vec![id.to_string()],
        HeaderValue::TextList(ids) => ids.iter().map(|id| id.to_string()).collect(),
        _ => vec![],
    };
    list.into_iter()
        .map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty() && id.len() <= 300 && !id.chars().any(char::is_control))
        .collect()
}

/// RFC 3834 automatic replies, bounces (an empty return path) and lists.
fn automated(message: &mail_parser::Message) -> bool {
    let header = |name: &str| {
        message
            .header_raw(name)
            .map(|v| v.trim().to_ascii_lowercase())
    };
    header("Auto-Submitted").is_some_and(|v| v != "no")
        || header("List-Id").is_some()
        || header("List-Unsubscribe").is_some()
        || header("Precedence")
            .is_some_and(|v| matches!(v.as_str(), "bulk" | "list" | "junk" | "auto_reply"))
        || header("X-Autoreply").is_some()
        || header("X-Autorespond").is_some()
        || header("X-Auto-Response-Suppress")
            .is_some_and(|v| v.contains("all") || v.contains("oof"))
        || header("Return-Path").is_some_and(|v| v == "<>")
}

/// The text above the quoted history and signature.
pub(crate) fn new_text(body: &str) -> String {
    let lines: Vec<&str> = body.lines().map(|l| l.trim_end()).collect();
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let wrote = trimmed.ends_with("wrote:")
            && (trimmed.starts_with("On ") || (i > 0 && lines[i - 1].trim().starts_with("On ")));
        let start_of_wrote = if wrote && !trimmed.starts_with("On ") {
            i - 1
        } else {
            i
        };
        let outlook = trimmed.starts_with("From:")
            && lines
                .get(i + 1..(i + 4).min(lines.len()))
                .is_some_and(|next| next.iter().any(|l| l.trim().starts_with("Sent:")));
        if wrote
            || outlook
            // The signature delimiter "-- ", its space already trimmed.
            || *line == "--"
            || trimmed.starts_with("-----Original Message-----")
            || (trimmed.len() >= 20 && trimmed.chars().all(|c| c == '_'))
        {
            end = if wrote { start_of_wrote } else { i };
            break;
        }
    }
    let mut kept: Vec<&str> = lines[..end].to_vec();
    while kept
        .last()
        .is_some_and(|l| l.trim().is_empty() || l.trim_start().starts_with('>'))
    {
        kept.pop();
    }
    let text = kept.join("\n").trim().to_owned();
    if text.is_empty() {
        body.trim().to_owned()
    } else {
        text
    }
}

fn bounded(text: &str) -> String {
    if text.len() <= MAX_TEXT {
        return text.to_owned();
    }
    let mut end = MAX_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n\n[The rest of this mail was too long to include.]",
        &text[..end]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPLY: &[u8] = b"From: Meera Iyer <Meera@Acme.example>\r\n\
To: nova.acme@mail.cuckoo.onl\r\n\
Subject: Re: Leave balance\r\n\
Message-ID: <reply-2@acme.example>\r\n\
In-Reply-To: <answer-1@mail.cuckoo.onl>\r\n\
References: <ask-1@acme.example> <answer-1@mail.cuckoo.onl>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
And how many sick days?\r\n\
\r\n\
On Mon, 9 Nov 2026 at 10:00, Nova <nova.acme@mail.cuckoo.onl> wrote:\r\n\
> You have 12 days of leave.\r\n";

    #[test]
    fn a_reply_keeps_its_new_text_sender_and_thread() {
        let mail = parse(REPLY).unwrap();
        assert_eq!(mail.from.as_deref(), Some("meera@acme.example"));
        assert_eq!(mail.subject, "Re: Leave balance");
        assert_eq!(mail.text, "And how many sick days?");
        assert_eq!(mail.message_id.as_deref(), Some("reply-2@acme.example"));
        assert_eq!(
            mail.parents,
            ["ask-1@acme.example", "answer-1@mail.cuckoo.onl"]
        );
        assert!(!mail.automated);
    }

    #[test]
    fn automatic_replies_bounces_and_lists_are_recognised() {
        for header in [
            "Auto-Submitted: auto-replied",
            "Precedence: bulk",
            "List-Id: <team.acme.example>",
            "Return-Path: <>",
            "X-Autoreply: yes",
        ] {
            let raw = format!(
                "From: a@acme.example\r\n{header}\r\nSubject: Out of office\r\n\r\nAway until Monday.\r\n"
            );
            assert!(parse(raw.as_bytes()).unwrap().automated, "{header}");
        }
        let raw = b"From: a@acme.example\r\nAuto-Submitted: no\r\nSubject: Hi\r\n\r\nHello\r\n";
        assert!(!parse(raw).unwrap().automated);
    }

    #[test]
    fn several_senders_and_html_only_mail() {
        let raw = b"From: a@acme.example, b@acme.example\r\nSubject: Hi\r\n\r\nHello\r\n";
        assert_eq!(parse(raw).unwrap().from, None);
        let html = b"From: a@acme.example\r\nSubject: Hi\r\nContent-Type: text/html\r\n\r\n<p>Please <b>book</b> a room</p>\r\n";
        assert!(parse(html).unwrap().text.contains("book"));
    }

    #[test]
    fn quoted_history_is_removed_in_the_common_shapes() {
        let outlook = "Yes, Friday works.\n\nFrom: Nova\nSent: Monday\nTo: Meera\nSubject: Meeting\n\nOld text";
        assert_eq!(new_text(outlook), "Yes, Friday works.");
        let wrapped =
            "Thanks!\n\nOn Mon, 9 Nov 2026 at 10:00, Nova\n<nova@mail.cuckoo.onl> wrote:\n> old";
        assert_eq!(new_text(wrapped), "Thanks!");
        let signature = "See you.\n-- \nMeera\nAcme";
        assert_eq!(new_text(signature), "See you.");
        assert_eq!(new_text("> only quoted"), "> only quoted");
    }

    #[test]
    fn very_long_mail_is_cut_on_a_character_boundary() {
        let text = "é".repeat(MAX_TEXT);
        let cut = bounded(&text);
        assert!(cut.len() < MAX_TEXT + 100 && cut.ends_with("include.]"));
    }
}
