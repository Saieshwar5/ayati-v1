//! A page as a Markdown file and back, for agents that open a page on their computer
//! and save it again. Everything a page can hold survives the round trip: headings,
//! paragraphs and line breaks, bullet, numbered and check lists (nested), quotes, code
//! blocks, rules, and bold, italic, strike, underline, code and links.
use serde_json::{Value, json};

const ESCAPED: &str = "\\*`[]~<";

/// The page as Markdown text.
pub fn to_markdown(document: &Value) -> String {
    let mut out = String::new();
    blocks(document, "", &mut out);
    out.trim_end().to_owned() + "\n"
}

fn children(node: &Value) -> impl Iterator<Item = &Value> {
    node["content"].as_array().into_iter().flatten()
}

fn blocks(node: &Value, indent: &str, out: &mut String) {
    for block in children(node) {
        match block["type"].as_str().unwrap_or("") {
            "heading" => {
                let level = block["attrs"]["level"].as_u64().unwrap_or(1).clamp(1, 3) as usize;
                out.push_str(&format!(
                    "{indent}{} {}\n\n",
                    "#".repeat(level),
                    inline(block)
                ));
            }
            "paragraph" => out.push_str(&format!("{indent}{}\n\n", guard(&inline(block)))),
            "bulletList" | "orderedList" | "taskList" => {
                list(block, indent, out);
                out.push('\n');
            }
            "blockquote" => {
                let mut inner = String::new();
                blocks(block, "", &mut inner);
                for line in inner.trim_end().lines() {
                    out.push_str(&format!("{indent}> {line}\n"));
                }
                out.push('\n');
            }
            "codeBlock" => {
                let text: String = children(block).filter_map(|n| n["text"].as_str()).collect();
                let language = block["attrs"]["language"].as_str().unwrap_or("");
                out.push_str(&format!("{indent}```{language}\n{text}\n{indent}```\n\n"));
            }
            "horizontalRule" => out.push_str(&format!("{indent}---\n\n")),
            _ => {}
        }
    }
}

fn list(node: &Value, indent: &str, out: &mut String) {
    let kind = node["type"].as_str().unwrap_or("");
    let first = node["attrs"]["start"].as_u64().unwrap_or(1);
    for (number, item) in (first..).zip(children(node)) {
        let marker = match kind {
            "orderedList" => format!("{number}. "),
            "taskList" if item["attrs"]["checked"] == true => "- [x] ".into(),
            "taskList" => "- [ ] ".into(),
            _ => "- ".into(),
        };
        let mut text = vec![];
        let deeper = format!("{indent}  ");
        let mut nested = String::new();
        for part in children(item) {
            match part["type"].as_str().unwrap_or("") {
                "bulletList" | "orderedList" | "taskList" => list(part, &deeper, &mut nested),
                _ => text.push(inline(part)),
            }
        }
        out.push_str(&format!("{indent}{marker}{}\n{nested}", text.join(" ")));
    }
}

/// A paragraph that would read as another kind of block keeps its first character literal.
fn guard(text: &str) -> String {
    let numbered = text
        .split_once(". ")
        .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    if numbered {
        return text.replacen(". ", "\\. ", 1);
    }
    if ["#", ">", "- ", "---"].iter().any(|p| text.starts_with(p)) {
        return format!("\\{text}");
    }
    text.to_owned()
}

fn inline(node: &Value) -> String {
    let mut out = String::new();
    for child in children(node) {
        if child["type"] == "hardBreak" {
            out.push_str("\\\n");
            continue;
        }
        let Some(text) = child["text"].as_str() else {
            continue;
        };
        let has = |name: &str| {
            child["marks"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|m| m["type"] == name)
        };
        // Markers hug the words: spaces around them stay outside.
        let core = text.trim();
        if core.is_empty() {
            out.push_str(text);
            continue;
        }
        let lead = &text[..text.len() - text.trim_start().len()];
        let trail = &text[text.trim_end().len()..];
        let mut shown = if has("code").is_some() {
            format!("`{core}`")
        } else {
            core.chars()
                .flat_map(|c| ESCAPED.contains(c).then_some('\\').into_iter().chain([c]))
                .collect()
        };
        for (mark, open, close) in [
            ("underline", "<u>", "</u>"),
            ("strike", "~~", "~~"),
            ("italic", "*", "*"),
            ("bold", "**", "**"),
        ] {
            if has(mark).is_some() {
                shown = format!("{open}{shown}{close}");
            }
        }
        if let Some(href) = has("link").and_then(|l| l["attrs"]["href"].as_str()) {
            shown = format!("[{shown}]({href})");
        }
        out.push_str(&format!("{lead}{shown}{trail}"));
    }
    out
}

/// Markdown text as a page.
pub fn from_markdown(input: &str) -> Value {
    let lines: Vec<&str> = input.lines().collect();
    let mut found = parse_blocks(&lines);
    if found.is_empty() {
        found.push(json!({"type":"paragraph"}));
    }
    json!({"type":"doc","content":found})
}

struct Marker<'a> {
    indent: usize,
    kind: &'static str,
    checked: Option<bool>,
    rest: &'a str,
}

fn marker(line: &str) -> Option<Marker<'_>> {
    let rest = line.trim_start();
    let indent = line.len() - rest.len();
    let item = |kind, checked, rest| {
        Some(Marker {
            indent,
            kind,
            checked,
            rest,
        })
    };
    if let Some(rest) = rest.strip_prefix("- [ ] ") {
        return item("taskList", Some(false), rest);
    }
    if let Some(rest) = rest
        .strip_prefix("- [x] ")
        .or_else(|| rest.strip_prefix("- [X] "))
    {
        return item("taskList", Some(true), rest);
    }
    if let Some(rest) = rest.strip_prefix("- ").or_else(|| rest.strip_prefix("* ")) {
        return item("bulletList", None, rest);
    }
    let (number, rest) = rest.split_once(". ")?;
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    item("orderedList", None, rest)
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let marks = line.chars().take_while(|c| *c == '#').count();
    ((1..=3).contains(&marks) && line[marks..].starts_with(' '))
        .then(|| (marks, line[marks..].trim()))
}

fn starts_block(line: &str) -> bool {
    let line = line.trim_start();
    line.is_empty()
        || line.starts_with("```")
        || line.starts_with('>')
        || line == "---"
        || heading(line).is_some()
        || marker(line).is_some()
}

fn paragraph(nodes: Vec<Value>) -> Value {
    if nodes.is_empty() {
        json!({"type":"paragraph"})
    } else {
        json!({"type":"paragraph","content":nodes})
    }
}

fn parse_blocks(lines: &[&str]) -> Vec<Value> {
    let mut found = vec![];
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at].trim();
        if line.is_empty() {
            at += 1;
        } else if let Some(language) = line.strip_prefix("```") {
            let end = (at + 1..lines.len())
                .find(|n| lines[*n].trim() == "```")
                .unwrap_or(lines.len());
            let text = lines[at + 1..end].join("\n");
            let language = language.trim();
            let mut block = json!({"type":"codeBlock"});
            if !language.is_empty()
                && language.len() < 40
                && language
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                block["attrs"] = json!({"language":language});
            }
            if !text.is_empty() {
                block["content"] = json!([{"type":"text","text":text}]);
            }
            found.push(block);
            at = end + 1;
        } else if let Some((level, text)) = heading(line) {
            let mut block = json!({"type":"heading","attrs":{"level":level}});
            let nodes = parse_inline(text, &[]);
            if !nodes.is_empty() {
                block["content"] = json!(nodes);
            }
            found.push(block);
            at += 1;
        } else if line == "---" {
            found.push(json!({"type":"horizontalRule"}));
            at += 1;
        } else if line.starts_with('>') {
            let end = (at..lines.len())
                .find(|n| !lines[*n].trim().starts_with('>'))
                .unwrap_or(lines.len());
            let inner: Vec<&str> = lines[at..end]
                .iter()
                .map(|l| {
                    let l = l.trim().trim_start_matches('>');
                    l.strip_prefix(' ').unwrap_or(l)
                })
                .collect();
            let mut content = parse_blocks(&inner);
            if content.is_empty() {
                content.push(json!({"type":"paragraph"}));
            }
            found.push(json!({"type":"blockquote","content":content}));
            at = end;
        } else if let Some(first) = marker(lines[at]) {
            found.push(parse_list(lines, &mut at, first.indent));
        } else {
            let mut nodes = vec![];
            loop {
                let text = lines[at].trim();
                let slashes = text.chars().rev().take_while(|c| *c == '\\').count();
                let broken = slashes % 2 == 1;
                nodes.extend(parse_inline(&text[..text.len() - usize::from(broken)], &[]));
                at += 1;
                let more = at < lines.len() && !starts_block(lines[at]);
                if broken && more {
                    nodes.push(json!({"type":"hardBreak"}));
                } else if more {
                    nodes.push(json!({"type":"text","text":" "}));
                }
                if !more {
                    break;
                }
            }
            found.push(paragraph(nodes));
        }
    }
    found
}

fn parse_list(lines: &[&str], at: &mut usize, indent: usize) -> Value {
    let kind = marker(lines[*at]).map_or("bulletList", |m| m.kind);
    let mut items = vec![];
    while *at < lines.len() {
        let Some(found) = marker(lines[*at]).filter(|m| m.indent == indent && m.kind == kind)
        else {
            break;
        };
        let mut content = vec![paragraph(parse_inline(found.rest.trim(), &[]))];
        *at += 1;
        if let Some(deeper) = lines
            .get(*at)
            .and_then(|l| marker(l))
            .filter(|m| m.indent > indent)
        {
            content.push(parse_list(lines, at, deeper.indent));
        }
        items.push(match found.checked {
            Some(checked) => {
                json!({"type":"taskItem","attrs":{"checked":checked},"content":content})
            }
            None => json!({"type":"listItem","content":content}),
        });
    }
    json!({"type":kind,"content":items})
}

/// The position of an unescaped `close` in `text` at or after `from`.
fn closing(text: &str, from: usize, close: &str) -> Option<usize> {
    let mut at = from;
    while at < text.len() {
        if text[at..].starts_with('\\') {
            at += 1 + text[at + 1..].chars().next().map_or(0, char::len_utf8);
        } else if text[at..].starts_with(close) {
            return Some(at);
        } else {
            at += text[at..].chars().next().map_or(1, char::len_utf8);
        }
    }
    None
}

fn link_allowed(href: &str) -> bool {
    href.len() <= 2048
        && !href.chars().any(|c| c.is_control() || c == ' ')
        && ["https://", "http://", "mailto:"]
            .iter()
            .any(|s| href.starts_with(s))
        && reqwest::Url::parse(href).is_ok()
}

fn parse_inline(text: &str, marks: &[Value]) -> Vec<Value> {
    let mut nodes: Vec<Value> = vec![];
    let mut plain = String::new();
    let flush = |plain: &mut String, nodes: &mut Vec<Value>| {
        if plain.is_empty() {
            return;
        }
        let mut node = json!({"type":"text","text":std::mem::take(plain)});
        if !marks.is_empty() {
            node["marks"] = json!(marks);
        }
        nodes.push(node);
    };
    let with = |mark: Value| {
        let mut all = marks.to_vec();
        all.push(mark);
        all
    };
    let mut at = 0;
    'scan: while at < text.len() {
        let rest = &text[at..];
        let next = rest.chars().next().unwrap_or(' ');
        if next == '\\' {
            if let Some(escaped) = rest[1..].chars().next().filter(char::is_ascii_punctuation) {
                plain.push(escaped);
                at += 1 + escaped.len_utf8();
                continue;
            }
        } else if next == '`' {
            if let Some(end) = rest[1..].find('`').filter(|end| *end > 0) {
                flush(&mut plain, &mut nodes);
                nodes.push(json!({"type":"text","text":&rest[1..=end],"marks":with(json!({"type":"code"}))}));
                at += end + 2;
                continue;
            }
        } else if next == '[' {
            let found = closing(rest, 1, "](")
                .and_then(|middle| closing(rest, middle + 2, ")").map(|end| (middle, end)));
            if let Some((middle, end)) = found
                && link_allowed(&rest[middle + 2..end])
            {
                flush(&mut plain, &mut nodes);
                let link = json!({"type":"link","attrs":{"href":&rest[middle + 2..end]}});
                nodes.extend(parse_inline(&rest[1..middle], &with(link)));
                at += end + 1;
                continue;
            }
        }
        for (open, close, mark) in [
            ("**", "**", "bold"),
            ("~~", "~~", "strike"),
            ("<u>", "</u>", "underline"),
            ("*", "*", "italic"),
        ] {
            if !rest.starts_with(open) {
                continue;
            }
            // Markers hug the words they mark; "2 * 3 * 4" stays as typed.
            let Some(end) = closing(rest, open.len(), close) else {
                continue;
            };
            let inner = &rest[open.len()..end];
            if inner.is_empty() || inner.starts_with(' ') || inner.ends_with(' ') {
                continue;
            }
            flush(&mut plain, &mut nodes);
            nodes.extend(parse_inline(inner, &with(json!({"type":mark}))));
            at += end + close.len();
            continue 'scan;
        }
        plain.push(next);
        at += next.len_utf8();
    }
    flush(&mut plain, &mut nodes);
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::library::model::document;

    #[test]
    fn everything_a_page_holds_survives_the_round_trip() {
        let page = json!({"type":"doc","content":[
            {"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Returns policy"}]},
            {"type":"paragraph","content":[
                {"type":"text","text":"Customers have "},
                {"type":"text","text":"30 days","marks":[{"type":"bold"}]},
                {"type":"text","text":" to return, see "},
                {"type":"text","text":"the form","marks":[{"type":"link","attrs":{"href":"https://example.test/form?a=1"}}]},
                {"type":"text","text":"."},
                {"type":"hardBreak"},
                {"type":"text","text":"Prices like 2 * 3 * 4 and snake_case_names stay as typed [sic]."}]},
            {"type":"bulletList","content":[
                {"type":"listItem","content":[
                    {"type":"paragraph","content":[{"type":"text","text":"Desks"}]},
                    {"type":"orderedList","content":[
                        {"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"Check the "},{"type":"text","text":"serial","marks":[{"type":"code"}]}]}]},
                        {"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"Refund","marks":[{"type":"italic"},{"type":"strike"}]}]}]}]}]},
                {"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"Chairs"}]}]}]},
            {"type":"taskList","content":[
                {"type":"taskItem","attrs":{"checked":true},"content":[{"type":"paragraph","content":[{"type":"text","text":"Told support"}]}]},
                {"type":"taskItem","attrs":{"checked":false},"content":[{"type":"paragraph","content":[{"type":"text","text":"Told sales"}]}]}]},
            {"type":"blockquote","content":[{"type":"paragraph","content":[{"type":"text","text":"Agreed","marks":[{"type":"underline"}]},{"type":"text","text":" on 5 November."}]}]},
            {"type":"codeBlock","attrs":{"language":"sql"},"content":[{"type":"text","text":"SELECT 1;\n-- done"}]},
            {"type":"horizontalRule"},
            {"type":"paragraph","content":[{"type":"text","text":"# not a heading"}]},
            {"type":"paragraph","content":[{"type":"text","text":"2024. a year, - not a list"}]}]});
        assert!(document(&page).is_ok());
        let text = to_markdown(&page);
        assert!(text.contains("Customers have **30 days** to return, see [the form](https://example.test/form?a=1).\\\n"), "{text}");
        assert!(
            text.contains("- Desks\n  1. Check the `serial`\n  2. *~~Refund~~*\n- Chairs\n"),
            "{text}"
        );
        assert!(
            text.contains("- [x] Told support\n- [ ] Told sales\n"),
            "{text}"
        );
        assert!(text.contains("> <u>Agreed</u> on 5 November.\n"), "{text}");
        assert!(text.contains("```sql\nSELECT 1;\n-- done\n```\n"), "{text}");
        let back = from_markdown(&text);
        assert!(document(&back).is_ok(), "{back}");
        assert_eq!(to_markdown(&back), text, "stable once written");
        assert_eq!(
            document(&back).unwrap(),
            document(&page).unwrap(),
            "same words"
        );
        let marks = |v: &Value| v.to_string().matches("\"marks\"").count();
        assert_eq!(marks(&back), marks(&page), "same formatting");
    }

    #[test]
    fn what_an_agent_writes_becomes_a_valid_page() {
        for text in [
            "",
            "# Quote\nFor Acme\ncontinued.\n\n- Desk: **1250**\n- [link](javascript:alert(1)) stays text\n",
            "**unclosed and `odd ~~ markers [x](y\n> \n```\nno end",
            "1. One\n   - deeper\n2. Two\n\nA * b * c",
        ] {
            let page = from_markdown(text);
            assert!(document(&page).is_ok(), "{text}: {page}");
        }
        let page = from_markdown("- [link](javascript:alert(1)) stays text");
        assert!(!page.to_string().contains("\"link\""));
    }
}
