//! Library documents cut into passages (LB2a): about a paragraph or short section each,
//! with the heading path they sit under, so search finds the right part and answers
//! cite it. Pages are read from their structure; prepared file text uses Markdown
//! heading lines ("# Returns", from Word and Excel) and PDF page breaks.
use serde_json::Value;

/// A passage closes at the first line break after this many characters…
const TARGET: usize = 1200;
/// …and no passage is longer than this.
const LONGEST: usize = 2000;
pub const MAX_PASSAGES: usize = 300;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Passage {
    pub section: String,
    pub text: String,
}

/// A page as text with Markdown heading lines.
pub fn page_text(document: &Value) -> String {
    fn inline(node: &Value, out: &mut String) {
        if let Some(text) = node["text"].as_str() {
            out.push_str(text);
        }
        if node["type"] == "hardBreak" {
            out.push(' ');
        }
        for child in node["content"].as_array().into_iter().flatten() {
            inline(child, out);
        }
    }
    fn blocks(node: &Value, out: &mut String) {
        match node["type"].as_str().unwrap_or("") {
            "heading" => {
                let level = node["attrs"]["level"].as_u64().unwrap_or(1).clamp(1, 3) as usize;
                let mut text = String::new();
                inline(node, &mut text);
                out.push_str(&format!("{} {}\n", "#".repeat(level), text.trim()));
            }
            "paragraph" | "codeBlock" => {
                let mut text = String::new();
                inline(node, &mut text);
                out.push_str(text.trim());
                out.push('\n');
            }
            _ => {
                for child in node["content"].as_array().into_iter().flatten() {
                    blocks(child, out);
                }
            }
        }
    }
    let mut out = String::new();
    blocks(document, &mut out);
    out
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let marks = line.chars().take_while(|c| *c == '#').count();
    let rest = line[marks..].strip_prefix(' ')?.trim();
    ((1..=3).contains(&marks) && !rest.is_empty()).then_some((marks, rest))
}

/// Pieces of one over-long line, broken after a sentence where possible.
fn pieces(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = line;
    while rest.chars().count() > LONGEST {
        let cut = rest
            .char_indices()
            .nth(LONGEST)
            .map_or(rest.len(), |(i, _)| i);
        let head = &rest[..cut];
        let at = head
            .rfind(". ")
            .filter(|i| *i > LONGEST / 2)
            .map_or(cut, |i| i + 2);
        out.push(rest[..at].trim().to_owned());
        rest = &rest[at..];
    }
    if !rest.trim().is_empty() {
        out.push(rest.trim().to_owned());
    }
    out
}

pub fn split(text: &str) -> Vec<Passage> {
    let mut passages = vec![];
    let mut path: [String; 3] = Default::default();
    let mut current = String::new();
    let paged = text.contains('\u{c}');
    let mut page = 1;
    let section = |path: &[String; 3]| {
        path.iter()
            .filter(|p| !p.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" › ")
    };
    if paged {
        path[0] = "Page 1".into();
    }
    let flush = |current: &mut String, path: &[String; 3], passages: &mut Vec<Passage>| {
        let text = current.trim();
        if !text.is_empty() {
            passages.push(Passage {
                section: section(path),
                text: text.to_owned(),
            });
        }
        current.clear();
    };
    for (index, chunk) in text.split('\u{c}').enumerate() {
        if index > 0 {
            flush(&mut current, &path, &mut passages);
            page += 1;
            path = [format!("Page {page}"), String::new(), String::new()];
        }
        for line in chunk.lines() {
            let line = line.trim_end();
            if let Some((level, name)) = heading(line) {
                flush(&mut current, &path, &mut passages);
                let base = usize::from(paged);
                let slot = (level - 1 + base).min(2);
                path[slot] = name.to_owned();
                for deeper in path.iter_mut().skip(slot + 1) {
                    deeper.clear();
                }
                continue;
            }
            for piece in pieces(line) {
                if current.chars().count() + piece.chars().count() > LONGEST {
                    flush(&mut current, &path, &mut passages);
                }
                current.push_str(&piece);
                current.push('\n');
                if current.chars().count() >= TARGET {
                    flush(&mut current, &path, &mut passages);
                }
            }
            if line.trim().is_empty() && current.chars().count() >= TARGET / 2 {
                flush(&mut current, &path, &mut passages);
            }
        }
    }
    flush(&mut current, &path, &mut passages);
    passages.truncate(MAX_PASSAGES);
    passages
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn headings_name_the_sections_their_passages_sit_under() {
        let text = "Intro line\n# Returns\nThe refund window is 30 days.\n## Exceptions\nSale items are final.\n# Shipping\nTwo days.";
        let got = split(text);
        assert_eq!(
            got,
            vec![
                Passage {
                    section: "".into(),
                    text: "Intro line".into()
                },
                Passage {
                    section: "Returns".into(),
                    text: "The refund window is 30 days.".into()
                },
                Passage {
                    section: "Returns › Exceptions".into(),
                    text: "Sale items are final.".into()
                },
                Passage {
                    section: "Shipping".into(),
                    text: "Two days.".into()
                },
            ]
        );
        assert_eq!(heading("#hashtag"), None);
        assert_eq!(heading("#### Too deep"), None);
    }

    #[test]
    fn long_text_is_cut_near_the_target_and_never_past_the_longest() {
        let sentence = "Each refund is checked by the support team before payment. ";
        let text = format!("{}\n\n{}", sentence.repeat(60), sentence.repeat(5));
        let got = split(&text);
        assert!(got.len() >= 3);
        assert!(got.iter().all(|p| p.text.chars().count() <= LONGEST));
        assert!(got[0].text.ends_with("payment."), "broken after a sentence");
        let joined: String = got
            .iter()
            .map(|p| p.text.replace('\n', " "))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(joined.matches("Each refund").count(), 65, "nothing is lost");
    }

    #[test]
    fn pdf_pages_are_sections_and_pages_are_read_from_their_structure() {
        let got = split("First page text\u{c}Second page\n# Terms\nPay in 30 days");
        assert_eq!(
            got.iter().map(|p| p.section.as_str()).collect::<Vec<_>>(),
            ["Page 1", "Page 2", "Page 2 › Terms"]
        );
        let page = json!({"type":"doc","content":[
            {"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Refunds"}]},
            {"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"30 days"}]}]}]},
            {"type":"paragraph","content":[{"type":"text","text":"Ask "},{"type":"text","marks":[{"type":"bold"}],"text":"Ravi"}]}]});
        assert_eq!(page_text(&page), "## Refunds\n30 days\nAsk Ravi\n");
        assert_eq!(split(&page_text(&page))[0].section, "Refunds");
    }
}
