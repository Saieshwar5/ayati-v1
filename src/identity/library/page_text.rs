//! Pages written by agents (Library LB3): plain text with Markdown headings ("# ",
//! "## ", "### ") and lists ("- ", "1. ") becomes the page structure people edit.
//! Inline formatting stays as typed; the result passes the same page validation.
use serde_json::{Value, json};

fn text(content: &str) -> Value {
    json!({"type":"paragraph","content":[{"type":"text","text":content}]})
}

pub fn document(input: &str) -> Value {
    let mut blocks: Vec<Value> = vec![];
    let mut list: Option<(&str, Vec<Value>)> = None;
    let close = |list: &mut Option<(&str, Vec<Value>)>, blocks: &mut Vec<Value>| {
        if let Some((kind, items)) = list.take() {
            blocks.push(json!({"type": kind, "content": items}));
        }
    };
    for line in input.lines() {
        let line = line.trim_end();
        let trimmed = line.trim_start();
        let marks = trimmed.chars().take_while(|c| *c == '#').count();
        let bullet = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "));
        let numbered = trimmed
            .split_once(". ")
            .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            .map(|(_, rest)| rest);
        if (1..=3).contains(&marks) && trimmed[marks..].starts_with(' ') {
            close(&mut list, &mut blocks);
            blocks.push(json!({"type":"heading","attrs":{"level":marks},"content":[{"type":"text","text":trimmed[marks..].trim()}]}));
        } else if let Some((kind, item)) = bullet
            .map(|b| ("bulletList", b))
            .or_else(|| numbered.map(|n| ("orderedList", n)))
            .filter(|(_, item)| !item.trim().is_empty())
        {
            if list.as_ref().is_some_and(|(k, _)| *k != kind) {
                close(&mut list, &mut blocks);
            }
            list.get_or_insert((kind, vec![]))
                .1
                .push(json!({"type":"listItem","content":[text(item.trim())]}));
        } else if trimmed.is_empty() {
            close(&mut list, &mut blocks);
        } else {
            close(&mut list, &mut blocks);
            blocks.push(text(trimmed));
        }
    }
    close(&mut list, &mut blocks);
    if blocks.is_empty() {
        blocks.push(json!({"type":"paragraph"}));
    }
    json!({"type":"doc","content":blocks})
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn headings_lists_and_paragraphs_become_a_valid_page() {
        let page = super::document(
            "# Quote for Acme\nPrepared on 5 November.\n\n- Standing desk: 1250\n- Chair: 300\n1. Sign\n2. Pay\nThanks.",
        );
        assert_eq!(
            page,
            json!({"type":"doc","content":[
                {"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Quote for Acme"}]},
                {"type":"paragraph","content":[{"type":"text","text":"Prepared on 5 November."}]},
                {"type":"bulletList","content":[
                    {"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"Standing desk: 1250"}]}]},
                    {"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"Chair: 300"}]}]}]},
                {"type":"orderedList","content":[
                    {"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"Sign"}]}]},
                    {"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"Pay"}]}]}]},
                {"type":"paragraph","content":[{"type":"text","text":"Thanks."}]}]})
        );
        assert!(crate::identity::library::model::document(&page).is_ok());
        assert!(crate::identity::library::model::document(&super::document("")).is_ok());
        assert!(
            crate::identity::library::model::document(&super::document(
                "#### not a heading\n2024. a year"
            ))
            .is_ok()
        );
    }
}
