//! The reply as a MIME message: plain text and simple HTML from the agent's
//! Markdown, threaded under the person's mail by `In-Reply-To`/`References`
//! and `Re:`, and marked `Auto-Submitted: auto-replied` so other machines don't
//! answer it (RFC 3834). Everything in it is chosen by Cuckoo; header values
//! are encoded, so no text can add a header or a recipient.
use base64::{Engine, engine::general_purpose::STANDARD};

pub(crate) struct Reply<'a> {
    pub from_name: &'a str,
    pub from: &'a str,
    pub to: &'a str,
    pub subject: &'a str,
    /// The person's mail being answered, without brackets.
    pub in_reply_to: Option<&'a str>,
    /// The thread so far, oldest first, without brackets.
    pub references: &'a [String],
    /// A unique value for this attempt: the boundary and the `Message-ID`.
    pub key: &'a str,
    pub domain: &'a str,
    pub markdown: &'a str,
    /// Seconds since 1970, for the `Date` header.
    pub now: u64,
}

pub(crate) fn compose(r: &Reply) -> String {
    let boundary = format!("cuckoo-{}", r.key);
    let mut headers = vec![
        format!("From: {} <{}>", encode(r.from_name), r.from),
        format!("To: <{}>", r.to),
        format!("Subject: {}", encode_text(&reply_subject(r.subject))),
        format!("Date: {}", date(r.now)),
        format!("Message-ID: <{}@{}>", r.key, r.domain),
    ];
    if let Some(parent) = r.in_reply_to {
        headers.push(format!("In-Reply-To: <{parent}>"));
    }
    if !r.references.is_empty() {
        let ids: Vec<String> = r.references.iter().map(|id| format!("<{id}>")).collect();
        headers.push(format!("References: {}", ids.join("\r\n ")));
    }
    headers.extend([
        "Auto-Submitted: auto-replied".to_owned(),
        "MIME-Version: 1.0".to_owned(),
        format!("Content-Type: multipart/alternative; boundary=\"{boundary}\""),
    ]);
    let mut out = headers.join("\r\n");
    out.push_str("\r\n\r\n");
    for (kind, body) in [
        ("text/plain", plain(r.markdown)),
        ("text/html", html(r.markdown)),
    ] {
        out.push_str(&format!(
            "--{boundary}\r\nContent-Type: {kind}; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n",
            wrapped(&STANDARD.encode(body.as_bytes()))
        ));
    }
    out.push_str(&format!("--{boundary}--\r\n"));
    out
}

/// `Re:` once, the person's subject otherwise unchanged.
pub(crate) fn reply_subject(subject: &str) -> String {
    let subject = subject.trim();
    if subject.is_empty() {
        "Re: Your message".to_owned()
    } else if subject.len() >= 3 && subject[..3].eq_ignore_ascii_case("re:") {
        subject.to_owned()
    } else {
        format!("Re: {subject}")
    }
}

/// RFC 2047 for anything that is not plain printable ASCII; always quoted.
fn encode(text: &str) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    if clean
        .chars()
        .all(|c| c.is_ascii() && c != '"' && c != '\\' && c != '=' && c != '?')
    {
        format!("\"{clean}\"")
    } else {
        format!("=?UTF-8?B?{}?=", STANDARD.encode(clean.as_bytes()))
    }
}

/// An unstructured header (the subject): as it is when plain printable ASCII,
/// RFC 2047 otherwise.
fn encode_text(text: &str) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    if clean.is_ascii() && !clean.contains("=?") {
        clean
    } else {
        format!("=?UTF-8?B?{}?=", STANDARD.encode(clean.as_bytes()))
    }
}

fn wrapped(base64: &str) -> String {
    base64
        .as_bytes()
        .chunks(76)
        .map(|c| std::str::from_utf8(c).expect("base64 is ASCII"))
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// The Markdown, readable as plain text: links as `text (url)`, no emphasis marks.
fn plain(markdown: &str) -> String {
    markdown
        .lines()
        .map(|line| {
            links(&line.replace("**", "").replace("__", ""), |t, u| {
                if t == u {
                    u.to_owned()
                } else {
                    format!("{t} ({u})")
                }
            })
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Simple, safe HTML: escaped text, paragraphs, lists, headings, bold, code
/// and http(s) links. Nothing from the reply becomes markup by itself.
fn html(markdown: &str) -> String {
    let mut out = String::from(
        "<!doctype html><html><body style=\"font-family:sans-serif;line-height:1.5\">",
    );
    let mut list: Option<&str> = None;
    let mut code = false;
    let mut paragraph: Vec<String> = vec![];
    let flush = |paragraph: &mut Vec<String>, out: &mut String| {
        if !paragraph.is_empty() {
            out.push_str(&format!("<p>{}</p>", paragraph.join("<br>")));
            paragraph.clear();
        }
    };
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            flush(&mut paragraph, &mut out);
            out.push_str(if code { "</pre>" } else { "<pre>" });
            code = !code;
            continue;
        }
        if code {
            out.push_str(&escape(line));
            out.push('\n');
            continue;
        }
        let item = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .map(|i| ("ul", i))
            .or_else(|| numbered(trimmed).map(|i| ("ol", i)));
        if list.is_some() && item.map(|(k, _)| k) != list {
            out.push_str(&format!("</{}>", list.take().unwrap()));
        }
        if let Some((kind, text)) = item {
            flush(&mut paragraph, &mut out);
            if list.is_none() {
                out.push_str(&format!("<{kind}>"));
                list = Some(kind);
            }
            out.push_str(&format!("<li>{}</li>", inline(text)));
        } else if trimmed.is_empty() {
            flush(&mut paragraph, &mut out);
        } else if let Some(title) = trimmed
            .trim_start_matches('#')
            .strip_prefix(' ')
            .filter(|_| trimmed.starts_with('#'))
        {
            flush(&mut paragraph, &mut out);
            out.push_str(&format!("<p><strong>{}</strong></p>", inline(title)));
        } else {
            paragraph.push(inline(line));
        }
    }
    flush(&mut paragraph, &mut out);
    if let Some(kind) = list {
        out.push_str(&format!("</{kind}>"));
    }
    if code {
        out.push_str("</pre>");
    }
    out.push_str("</body></html>");
    out
}

fn numbered(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0).then(|| line[digits..].strip_prefix(". "))?
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Bold, inline code and links inside one escaped line.
fn inline(line: &str) -> String {
    // The link markers below are control characters a reply can't contain.
    let line: String = line
        .chars()
        .filter(|c| !matches!(c, '\u{1}'..='\u{3}'))
        .collect();
    let linked = links(&line, |text, url| {
        if url.starts_with("https://") || url.starts_with("http://") {
            format!("\u{1}{}\u{2}{}\u{3}", url, text)
        } else {
            format!("{text} ({url})")
        }
    });
    let mut out = escape(&linked);
    out = pairs(&out, "**", "<strong>", "</strong>");
    out = pairs(&out, "`", "<code>", "</code>");
    // Links were marked before escaping, so their text and URL are escaped too.
    while let (Some(a), Some(b), Some(c)) =
        (out.find('\u{1}'), out.find('\u{2}'), out.find('\u{3}'))
    {
        if !(a < b && b < c) {
            break;
        }
        let url = out[a + 1..b].to_owned();
        let text = out[b + 1..c].to_owned();
        out.replace_range(a..=c, &format!("<a href=\"{url}\">{text}</a>"));
    }
    out.replace(['\u{1}', '\u{2}', '\u{3}'], "")
}

fn pairs(text: &str, mark: &str, open: &str, close: &str) -> String {
    let parts: Vec<&str> = text.split(mark).collect();
    if parts.len() < 3 {
        return text.to_owned();
    }
    let mut out = String::new();
    let complete = (parts.len() - 1) / 2 * 2;
    for (i, part) in parts.iter().enumerate() {
        out.push_str(part);
        if i < parts.len() - 1 {
            out.push_str(if i >= complete {
                mark
            } else if i % 2 == 0 {
                open
            } else {
                close
            });
        }
    }
    out
}

/// `[text](url)` replaced by `render(text, url)`.
fn links(line: &str, render: impl Fn(&str, &str) -> String) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let link = after.find("](").and_then(|close| {
            let target = &after[close + 2..];
            target.find(')').map(|end| (close, &target[..end]))
        });
        match link {
            Some((close, url)) if !url.is_empty() && !url.contains(' ') => {
                out.push_str(&rest[..open]);
                out.push_str(&render(&after[..close], url));
                rest = &after[close + 2 + url.len() + 1..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// RFC 5322 date in UTC, for example `Sat, 14 Nov 2026 10:00:00 +0000`.
fn date(now: u64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (now / 86_400) as i64;
    let seconds = now % 86_400;
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{}, {day} {} {year} {:02}:{:02}:{:02} +0000",
        DAYS[(days % 7) as usize],
        MONTHS[(month - 1) as usize],
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(markdown: &str) -> String {
        compose(&Reply {
            from_name: "Nova",
            from: "nova.acme@mail.cuckoo.onl",
            to: "meera@acme.example",
            subject: "Leave balance",
            in_reply_to: Some("ask-1@acme.example"),
            references: &["ask-1@acme.example".to_owned()],
            key: "k1",
            domain: "mail.cuckoo.onl",
            markdown,
            now: 1_794_650_400,
        })
    }

    #[test]
    fn the_reply_is_threaded_marked_automatic_and_parses_back() {
        let raw = reply(
            "You have **12** days.\n\n- Annual: 10\n- Casual: 2\n\n[Policy](https://acme.example/leave)",
        );
        assert!(raw.contains("In-Reply-To: <ask-1@acme.example>\r\n"));
        assert!(raw.contains("Subject: Re: Leave balance\r\n"));
        assert!(raw.contains("Auto-Submitted: auto-replied\r\n"));
        assert!(raw.contains("Date: Sat, 14 Nov 2026 10:00:00 +0000\r\n"));
        let parsed = mail_parser::MessageParser::default()
            .parse(raw.as_bytes())
            .unwrap();
        assert_eq!(parsed.subject(), Some("Re: Leave balance"));
        assert_eq!(
            parsed.to().unwrap().first().unwrap().address(),
            Some("meera@acme.example")
        );
        assert_eq!(parsed.message_id(), Some("k1@mail.cuckoo.onl"));
        let text = parsed.body_text(0).unwrap();
        assert!(
            text.contains("You have 12 days.")
                && text.contains("Policy (https://acme.example/leave)")
        );
        let html = parsed.body_html(0).unwrap();
        assert!(html.contains("<strong>12</strong>") && html.contains("<li>Annual: 10</li>"));
        assert!(html.contains("<a href=\"https://acme.example/leave\">Policy</a>"));
    }

    #[test]
    fn nothing_in_the_reply_becomes_markup_or_a_header() {
        let raw = reply(
            "<script>alert(1)</script> [x](javascript:alert(1)) \"q\"\r\nBcc: evil@example.com",
        );
        let parsed = mail_parser::MessageParser::default()
            .parse(raw.as_bytes())
            .unwrap();
        assert!(parsed.bcc().is_none());
        let html = parsed.body_html(0).unwrap();
        assert!(!html.contains("<script>") && !html.contains("href=\"javascript"));
        let header = encode("Nova\r\nBcc: evil@example.com");
        assert!(!header.contains('\n'));
        assert_eq!(encode("Növa"), "=?UTF-8?B?TsO2dmE=?=");
    }

    #[test]
    fn subjects_get_one_re() {
        assert_eq!(reply_subject("RE: Leave"), "RE: Leave");
        assert_eq!(reply_subject("Leave"), "Re: Leave");
        assert_eq!(reply_subject("  "), "Re: Your message");
    }
}
