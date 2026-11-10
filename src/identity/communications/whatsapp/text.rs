//! Agent replies are Markdown; WhatsApp has its own small formatting
//! (`*bold*`, `_italic_`, `~strike~`, ``` code ```). Long replies are split on
//! line boundaries into parts the provider accepts.
use anyhow::{Result, ensure};

/// Twilio accepts up to 1,600 characters in one WhatsApp message body.
pub(crate) const PART_CHARS: usize = 1600;
/// A 12,000-byte reply fits in eight parts.
pub(crate) const MAX_PARTS: usize = 8;

/// WhatsApp formatting for an agent's Markdown reply.
pub(crate) fn whatsapp(markdown: &str) -> String {
    let mut out = Vec::new();
    let mut code = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            code = !code;
            out.push("```".to_owned());
            continue;
        }
        if code {
            out.push(line.to_owned());
            continue;
        }
        let trimmed = line.trim_start();
        let line = if let Some(title) = heading(trimmed) {
            format!("*{}*", inline(title))
        } else if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            format!("• {}", inline(item))
        } else {
            inline(line)
        };
        out.push(line);
    }
    out.join("\n").trim().to_owned()
}

fn heading(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    (1..=6)
        .contains(&hashes)
        .then(|| line[hashes..].strip_prefix(' '))?
}

/// Bold, strike and links within one line. `**x**`/`__x__` become `*x*`/`_x_`,
/// `~~x~~` becomes `~x~`, and `[text](url)` becomes `text (url)`.
fn inline(line: &str) -> String {
    let line = line
        .replace("**", "*")
        .replace("__", "_")
        .replace("~~", "~");
    let mut out = String::with_capacity(line.len());
    let mut rest = line.as_str();
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let link = after.find("](").and_then(|close| {
            let target = &after[close + 2..];
            target.find(')').map(|end| (close, &target[..end]))
        });
        match link {
            Some((close, url)) if !url.is_empty() && !url.contains(' ') => {
                out.push_str(&rest[..open]);
                let text = &after[..close];
                if text == url {
                    out.push_str(url);
                } else {
                    out.push_str(&format!("{text} ({url})"));
                }
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

/// Splits a message into parts of at most `PART_CHARS` characters, preferring
/// line breaks. Refuses a message that would need more than `MAX_PARTS`.
pub(crate) fn parts(text: &str) -> Result<Vec<String>> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let push = |current: &mut String, parts: &mut Vec<String>| {
        let part = current.trim();
        if !part.is_empty() {
            parts.push(part.to_owned());
        }
        current.clear();
    };
    for line in text.split_inclusive('\n') {
        if current.chars().count() + line.chars().count() > PART_CHARS {
            push(&mut current, &mut parts);
        }
        // A line longer than a part is cut at character boundaries.
        let mut line = line;
        while line.chars().count() > PART_CHARS {
            let cut = line
                .char_indices()
                .nth(PART_CHARS)
                .map(|(i, _)| i)
                .unwrap_or(line.len());
            current.push_str(&line[..cut]);
            push(&mut current, &mut parts);
            line = &line[cut..];
        }
        current.push_str(line);
    }
    push(&mut current, &mut parts);
    ensure!(!parts.is_empty(), "The reply is empty");
    ensure!(
        parts.len() <= MAX_PARTS,
        "The reply is too long for WhatsApp"
    );
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_becomes_whatsapp_formatting() {
        let reply = "# Leave\n\nYou have **12 days** left.\n- Casual: 4\n* Sick: 8\n\nSee [the policy](https://example.com/leave) or https://example.com.\n```\n**kept** [as](is)\n```";
        assert_eq!(
            whatsapp(reply),
            "*Leave*\n\nYou have *12 days* left.\n• Casual: 4\n• Sick: 8\n\nSee the policy (https://example.com/leave) or https://example.com.\n```\n**kept** [as](is)\n```"
        );
        assert_eq!(whatsapp("[a] and [b](c d)"), "[a] and [b](c d)");
        assert_eq!(whatsapp("~~old~~ [x](x)"), "~old~ x");
    }

    #[test]
    fn long_replies_split_on_lines_within_the_limit() {
        assert_eq!(parts("Hello").unwrap(), vec!["Hello"]);
        let line = format!("{}\n", "a".repeat(1000));
        let split = parts(&line.repeat(3)).unwrap();
        assert_eq!(split.len(), 3);
        assert!(split.iter().all(|p| p.chars().count() == 1000));
        let long = "é".repeat(PART_CHARS * 2 + 5);
        let split = parts(&long).unwrap();
        assert_eq!(
            split.iter().map(|p| p.chars().count()).collect::<Vec<_>>(),
            vec![1600, 1600, 5]
        );
        assert!(parts(&"b".repeat(PART_CHARS * MAX_PARTS + 1)).is_err());
        assert!(parts("  \n ").is_err());
    }
}
