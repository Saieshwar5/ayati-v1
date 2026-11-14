//! Agent replies are Markdown; Telegram reads a small HTML subset (`<b>`,
//! `<s>`, `<code>`, `<pre>`, `<a>`). Everything else is escaped, so a reply
//! can never inject markup. Long replies are split on line boundaries.
use anyhow::{Result, ensure};

/// Telegram accepts 4,096 characters after entities are parsed; counting the
/// markup too keeps every part safely under it.
pub(crate) const PART_CHARS: usize = 4000;
/// A 12,000-byte reply fits in eight parts.
pub(crate) const MAX_PARTS: usize = 8;

struct Line {
    html: String,
    code: bool,
}

/// The reply as Telegram HTML parts, each with its code blocks closed.
pub(crate) fn parts(markdown: &str) -> Result<Vec<String>> {
    let lines = render(markdown);
    let mut parts: Vec<String> = Vec::new();
    let mut current: Vec<&Line> = Vec::new();
    let mut size = 0;
    for line in &lines {
        let length = line.html.chars().count() + 12;
        if size + length > PART_CHARS && !current.is_empty() {
            parts.push(join(&current));
            current.clear();
            size = 0;
        }
        current.push(line);
        size += length;
    }
    if !current.is_empty() {
        parts.push(join(&current));
    }
    parts.retain(|p| !p.trim().is_empty());
    ensure!(!parts.is_empty(), "The reply is empty");
    ensure!(
        parts.len() <= MAX_PARTS,
        "The reply is too long for Telegram"
    );
    Ok(parts)
}

/// The same part without markup, for when Telegram refuses the formatting.
pub(crate) fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

fn join(lines: &[&Line]) -> String {
    let mut out = String::new();
    let mut code = false;
    for (i, line) in lines.iter().enumerate() {
        if line.code && !code {
            out.push_str("<pre>");
        } else if !line.code && code {
            out.push_str("</pre>");
        }
        if i > 0 && !(line.code && !code) {
            out.push('\n');
        }
        code = line.code;
        out.push_str(&line.html);
    }
    if code {
        out.push_str("</pre>");
    }
    out.trim().to_owned()
}

/// A line too long for one part loses its formatting and is cut into pieces
/// that fit once escaped.
fn cut(line: Line, out: &mut Vec<Line>) {
    if line.html.chars().count() + 12 <= PART_CHARS {
        out.push(line);
        return;
    }
    let mut piece = String::new();
    for c in plain(&line.html).chars() {
        piece.push(c);
        if escape(&piece).chars().count() + 12 > PART_CHARS - 100 {
            out.push(Line {
                html: escape(&piece),
                code: line.code,
            });
            piece.clear();
        }
    }
    if !piece.is_empty() {
        out.push(Line {
            html: escape(&piece),
            code: line.code,
        });
    }
}

fn render(markdown: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for line in render_lines(markdown) {
        cut(line, &mut lines);
    }
    lines
}

fn render_lines(markdown: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut code = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            code = !code;
            continue;
        }
        if code {
            out.push(Line {
                html: escape(line),
                code: true,
            });
            continue;
        }
        let trimmed = line.trim_start();
        let html = if let Some(title) = heading(trimmed) {
            format!("<b>{}</b>", inline(title))
        } else if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            format!("• {}", inline(item))
        } else {
            inline(line)
        };
        out.push(Line { html, code: false });
    }
    out
}

fn heading(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    (1..=6)
        .contains(&hashes)
        .then(|| line[hashes..].strip_prefix(' '))?
}

pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Code spans, bold, strike and links within one line. A marker without its
/// closing pair stays as text.
fn inline(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('`')
            && let Some(end) = after.find('`')
        {
            out.push_str(&format!("<code>{}</code>", escape(&after[..end])));
            rest = &after[end + 1..];
            continue;
        }
        let mut styled = false;
        for (marker, tag) in [("**", "b"), ("__", "b"), ("~~", "s")] {
            if let Some(after) = rest.strip_prefix(marker)
                && let Some(end) = after.find(marker)
                && end > 0
            {
                out.push_str(&format!("<{tag}>{}</{tag}>", inline(&after[..end])));
                rest = &after[end + marker.len()..];
                styled = true;
                break;
            }
        }
        if styled {
            continue;
        }
        if let Some(after) = rest.strip_prefix('[')
            && let Some(close) = after.find("](")
            && let Some(end) = after[close + 2..].find(')')
        {
            let url = &after[close + 2..close + 2 + end];
            if safe_url(url) {
                out.push_str(&format!(
                    "<a href=\"{}\">{}</a>",
                    escape(url),
                    escape(&after[..close])
                ));
                rest = &after[close + 3 + end..];
                continue;
            }
        }
        let c = rest.chars().next().unwrap();
        out.push_str(&escape(&c.to_string()));
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn safe_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(|c| c.is_whitespace() || c == '"')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_becomes_telegram_html_and_text_is_escaped() {
        let reply = "# Leave\n\nYou have **12 days** left & <counting>.\n- Casual: `4`\n* Sick: ~~8~~ 7\nSee [the policy](https://example.com/leave?a=1&b=2).\n```\n**kept** <as> is\n```";
        assert_eq!(
            parts(reply).unwrap(),
            vec![
                "<b>Leave</b>\n\nYou have <b>12 days</b> left &amp; &lt;counting&gt;.\n• Casual: <code>4</code>\n• Sick: <s>8</s> 7\nSee <a href=\"https://example.com/leave?a=1&amp;b=2\">the policy</a>.<pre>**kept** &lt;as&gt; is</pre>"
            ]
        );
        // Unpaired markers and unsafe links stay as text.
        assert_eq!(
            parts("2 ** 3 and [x](javascript:alert(1))").unwrap(),
            vec!["2 ** 3 and [x](javascript:alert(1))"]
        );
        assert_eq!(plain("<b>a</b> &amp; &lt;b&gt;"), "a & <b>");
    }

    #[test]
    fn long_replies_split_on_lines_and_keep_code_blocks_closed() {
        let line = format!("{}\n", "a".repeat(1500));
        let split = parts(&line.repeat(4)).unwrap();
        assert_eq!(split.len(), 2);
        let code = format!("```\n{}```", line.repeat(4));
        let split = parts(&code).unwrap();
        assert_eq!(split.len(), 2);
        assert!(
            split
                .iter()
                .all(|p| p.starts_with("<pre>") && p.ends_with("</pre>"))
        );
        // One very long line is cut into pieces that still fit.
        let long = parts(&"a".repeat(PART_CHARS * 2)).unwrap();
        assert_eq!(long.len(), 3);
        assert!(long.iter().all(|p| p.chars().count() <= PART_CHARS));
        assert!(parts(&format!("{}\n", "c".repeat(3000)).repeat(9)).is_err());
        assert!(parts("  \n ").is_err());
    }
}
