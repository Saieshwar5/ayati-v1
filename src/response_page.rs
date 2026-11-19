//! Pages inside agent replies. An agent may put one long, read-only document in
//! its reply between `<page title="…">` and `</page>`, each on a line of its own.
//! Chat shows the words around it and a card; the page opens in its own reading
//! view. A page is part of the saved reply: it has the reply's access and version,
//! and nothing here stores, grants or runs anything. `web/src/react/pages/
//! page-format.ts` reads the same format.
use serde_json::{Value, json};
use std::borrow::Cow;

pub(crate) const MAX_TITLE: usize = 120;
const WORDS_PER_MINUTE: usize = 200;

pub(crate) struct Page<'a> {
    pub title: &'a str,
    pub body: &'a str,
    /// False while the agent is still writing it (no closing line yet).
    pub complete: bool,
}
pub(crate) struct Reply<'a> {
    /// The reply without its page.
    pub text: Cow<'a, str>,
    pub page: Option<Page<'a>>,
}

fn title(line: &str) -> Option<&str> {
    let title = line
        .trim_end()
        .strip_prefix("<page title=\"")?
        .strip_suffix("\">")?;
    let fits = !title.trim().is_empty()
        && title.chars().count() <= MAX_TITLE
        && !title.contains(['"', '<', '>']);
    fits.then_some(title)
}

/// Splits a reply into its words and its first page. Later pages stay as text.
pub(crate) fn split(text: &str) -> Reply<'_> {
    let mut offset = 0;
    let mut open = None;
    for line in text.split_inclusive('\n') {
        let end = offset + line.len();
        match open {
            None => {
                if let Some(title) = title(line) {
                    open = Some((title, offset, end));
                }
            }
            Some((title, start, body)) if line.trim() == "</page>" => {
                let page = Page {
                    title,
                    body: text[body..offset].trim_matches('\n'),
                    complete: true,
                };
                return Reply {
                    text: around(&text[..start], &text[end..]),
                    page: Some(page),
                };
            }
            Some(_) => {}
        }
        offset = end;
    }
    match open {
        Some((title, start, body)) => Reply {
            text: around(&text[..start], ""),
            page: Some(Page {
                title,
                body: text[body..].trim_matches('\n'),
                complete: false,
            }),
        },
        None => Reply {
            text: Cow::Borrowed(text),
            page: None,
        },
    }
}

fn around<'a>(before: &'a str, after: &str) -> Cow<'a, str> {
    let (before, after) = (before.trim_end(), after.trim());
    if after.is_empty() {
        Cow::Borrowed(before)
    } else if before.is_empty() {
        Cow::Owned(after.to_owned())
    } else {
        Cow::Owned(format!("{before}\n\n{after}"))
    }
}

impl Page<'_> {
    /// What the card in Chat says about the page, without its text.
    pub(crate) fn summary(&self) -> Value {
        let (mut sections, mut words, mut fenced) = (0, 0, false);
        let mut blocks: Vec<&str> = Vec::new();
        let mut has = |kind: &'static str| {
            if !blocks.contains(&kind) {
                blocks.push(kind);
            }
        };
        for line in self.body.lines() {
            let line = line.trim_end();
            if let Some(kind) = line.strip_prefix("```") {
                fenced = !fenced;
                match kind.trim() {
                    "chart" if fenced => has("chart"),
                    "steps" if fenced => has("diagram"),
                    _ => {}
                }
                continue;
            }
            if fenced {
                continue;
            }
            if line.starts_with("## ") {
                sections += 1;
            } else if line.starts_with('|') {
                has("table");
            }
            words += line.split_whitespace().count();
        }
        json!({"title":self.title,"sections":sections,"blocks":blocks,"complete":self.complete,
            "minutes":words.div_ceil(WORDS_PER_MINUTE).max(1)})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_taken_out_of_the_reply_and_described() {
        let text = "Courier B wins.\n\n<page title=\"Courier comparison\">\n## Cost\nCheapest.\n\n```chart\n{\"title\":\"Cost\"}\n```\n\n## Side by side\n| A | B |\n|---|---|\n| 1 | 2 |\n\n```steps\nSign | Trial\n```\n</page>\n\nTell me if you want the plan.";
        let reply = split(text);
        assert_eq!(
            reply.text,
            "Courier B wins.\n\nTell me if you want the plan."
        );
        let page = reply.page.unwrap();
        assert_eq!(page.title, "Courier comparison");
        assert!(page.body.starts_with("## Cost") && page.body.ends_with("```"));
        assert_eq!(
            page.summary(),
            json!({"title":"Courier comparison","sections":2,"blocks":["chart","table","diagram"],"complete":true,"minutes":1})
        );
    }

    #[test]
    fn a_page_still_being_written_is_not_shown_as_reply_text() {
        let reply = split("Here it is.\n<page title=\"Draft\">\n## One\nSo far");
        assert_eq!(reply.text, "Here it is.");
        let page = reply.page.unwrap();
        assert_eq!((page.body, page.complete), ("## One\nSo far", false));
        assert_eq!(page.summary()["complete"], false);
    }

    #[test]
    fn ordinary_replies_and_malformed_page_lines_stay_as_written() {
        for text in [
            "A short answer",
            "Use <page title=\"x\"> inline to make one.",
            "<page title=\"\">\nNo title\n</page>",
            "<page title=\"a\"b\">\nBad title\n</page>",
            "</page>",
        ] {
            let reply = split(text);
            assert!(reply.page.is_none(), "{text}");
            assert_eq!(reply.text, text);
        }
        let long = format!(
            "<page title=\"{}\">\nText\n</page>",
            "t".repeat(MAX_TITLE + 1)
        );
        assert!(split(&long).page.is_none());
        let only = split("<page title=\"Only\">\nText\n</page>");
        assert_eq!(only.text, "");
        assert_eq!(only.page.unwrap().body, "Text");
    }
}
