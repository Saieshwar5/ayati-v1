use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_FILE: usize = 25 * 1024 * 1024;
pub const WORKSPACE_QUOTA: i64 = 1024 * 1024 * 1024;

#[derive(Debug)]
pub struct Denied;
impl std::fmt::Display for Denied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Library item unavailable or access changed")
    }
}
impl std::error::Error for Denied {}
#[derive(Debug)]
pub struct Conflict;
impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "This item changed. Reload the saved version before trying again; your draft is preserved."
        )
    }
}
impl std::error::Error for Conflict {}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavePage {
    pub id: String,
    pub request: String,
    pub revision: i64,
    pub title: String,
    pub document: Value,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersonGrant {
    pub user: String,
    pub access: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShareItem {
    pub request: String,
    pub revision: i64,
    pub visibility: String,
    pub people: Vec<PersonGrant>,
    pub agents: Vec<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ItemAction {
    pub request: String,
    pub revision: i64,
    pub action: String,
    #[serde(default)]
    pub version: Option<i64>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UploadFile {
    pub id: String,
    pub request: String,
    pub title: String,
    pub size: i64,
    pub checksum: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AskItem {
    pub request: String,
    pub revision: i64,
    pub agent: String,
    pub question: String,
}
pub fn uuid(id: &str) -> Result<()> {
    ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid Library ID");
    Ok(())
}
pub fn title(title: &str) -> Result<()> {
    ensure!(
        !title.trim().is_empty() && title.len() <= 180 && !title.chars().any(char::is_control),
        "Use a title between 1 and 180 bytes without control characters"
    );
    Ok(())
}
pub fn document(value: &Value) -> Result<String> {
    ensure!(
        value.to_string().len() <= 256 * 1024 && value["type"] == "doc",
        "Page must be a document up to 256 KiB"
    );
    let mut text = String::new();
    let mut nodes = 0;
    visit(value, 0, &mut nodes, &mut text)?;
    Ok(text)
}
fn visit(value: &Value, depth: usize, nodes: &mut usize, text: &mut String) -> Result<()> {
    *nodes += 1;
    ensure!(depth <= 20 && *nodes <= 5000, "Page is too complex");
    let kind = value["type"].as_str().unwrap_or("");
    ensure!(
        [
            "doc",
            "paragraph",
            "text",
            "heading",
            "bulletList",
            "orderedList",
            "listItem",
            "taskList",
            "taskItem",
            "blockquote",
            "codeBlock",
            "hardBreak",
            "horizontalRule"
        ]
        .contains(&kind),
        "Unsupported document block"
    );
    ensure!(
        value.as_object().is_some_and(|v| v
            .keys()
            .all(|k| ["type", "text", "content", "marks", "attrs"].contains(&k.as_str()))),
        "Unsupported document properties"
    );
    if kind == "text" {
        text.push_str(
            value["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid text block"))?,
        );
    }
    if let Some(attrs) = value.get("attrs") {
        ensure!(
            attrs.is_null()
                || attrs
                    .as_object()
                    .is_some_and(|v| v.keys().all(|k| match kind {
                        "heading" => k == "level",
                        "orderedList" => k == "start" || k == "type",
                        "taskItem" => k == "checked",
                        "codeBlock" => k == "language",
                        _ => false,
                    })),
            "Unsupported block attributes"
        );
        if kind == "heading" {
            ensure!(
                matches!(attrs["level"].as_i64(), Some(1..=3)),
                "Use heading levels 1 to 3"
            );
        }
        if kind == "codeBlock" {
            ensure!(
                attrs["language"].is_null()
                    || attrs["language"].as_str().is_some_and(|s| s.len() < 40
                        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')),
                "Invalid code language"
            );
        }
    }
    if let Some(marks) = value.get("marks") {
        let marks = marks
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid formatting"))?;
        ensure!(marks.len() <= 8, "Too much formatting");
        for mark in marks {
            ensure!(
                mark.as_object()
                    .is_some_and(|m| m.keys().all(|k| ["type", "attrs"].contains(&k.as_str()))),
                "Unsupported formatting properties"
            );
            let name = mark["type"].as_str().unwrap_or("");
            ensure!(
                ["bold", "italic", "strike", "code", "underline", "link"].contains(&name),
                "Unsupported formatting"
            );
            if name == "link" {
                ensure!(
                    mark["attrs"].as_object().is_some_and(|a| a
                        .keys()
                        .all(|k| ["href", "target", "rel", "class"].contains(&k.as_str()))),
                    "Unsupported link attributes"
                );
                ensure!(
                    mark["attrs"]["target"].is_null()
                        || matches!(mark["attrs"]["target"].as_str(), Some("_blank" | "_self")),
                    "Unsupported link target"
                );
                ensure!(
                    mark["attrs"]["class"].is_null(),
                    "Custom link classes are unavailable"
                );
                let href = mark["attrs"]["href"].as_str().unwrap_or("");
                let url = reqwest::Url::parse(href)?;
                ensure!(
                    href.len() <= 2048
                        && !href.chars().any(char::is_control)
                        && ["https", "http", "mailto"].contains(&url.scheme()),
                    "Use an http, https or email link"
                );
            } else {
                ensure!(
                    mark.get("attrs").is_none_or(
                        |a| a.is_null() || a.as_object().is_some_and(serde_json::Map::is_empty)
                    ),
                    "Unsupported formatting attributes"
                );
            }
        }
    }
    let children = value
        .get("content")
        .map(|v| {
            v.as_array()
                .ok_or_else(|| anyhow::anyhow!("Invalid document content"))
        })
        .transpose()?;
    validate_children(kind, children.map(Vec::as_slice).unwrap_or_default())?;
    if let Some(content) = children {
        for child in content {
            visit(child, depth + 1, nodes, text)?;
        }
    }
    if ["paragraph", "heading", "hardBreak", "codeBlock"].contains(&kind) {
        text.push('\n');
    }
    Ok(())
}

fn validate_children(kind: &str, children: &[Value]) -> Result<()> {
    let block = |name: &str| {
        [
            "paragraph",
            "heading",
            "bulletList",
            "orderedList",
            "taskList",
            "blockquote",
            "codeBlock",
            "horizontalRule",
        ]
        .contains(&name)
    };
    if [
        "doc",
        "blockquote",
        "bulletList",
        "orderedList",
        "listItem",
        "taskList",
        "taskItem",
    ]
    .contains(&kind)
    {
        ensure!(!children.is_empty(), "This document block requires content");
    }
    if kind == "listItem" || kind == "taskItem" {
        ensure!(
            children.first().is_some_and(|c| c["type"] == "paragraph"),
            "List items must start with a paragraph"
        );
    }
    for child in children {
        let child_kind = child["type"].as_str().unwrap_or("");
        let valid = match kind {
            "doc" | "blockquote" | "listItem" | "taskItem" => block(child_kind),
            "paragraph" | "heading" => ["text", "hardBreak"].contains(&child_kind),
            "bulletList" | "orderedList" => child_kind == "listItem",
            "taskList" => child_kind == "taskItem",
            "codeBlock" => {
                child_kind == "text"
                    && child
                        .get("marks")
                        .is_none_or(|m| m.as_array().is_some_and(Vec::is_empty))
            }
            _ => false,
        };
        ensure!(valid, "Invalid document block nesting");
    }
    Ok(())
}
pub fn checksum(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn validates_documents_and_rejects_active_content() {
        assert_eq!(document(&json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Plan"}]}]})).unwrap(), "Plan\n");
        assert!(document(&json!({"type":"doc","content":[{"type":"iframe"}]})).is_err());
        assert!(document(&json!({"type":"doc","content":[{"type":"text","text":"x","marks":[{"type":"link","attrs":{"href":"javascript:alert(1)"}}]}]})).is_err());
        assert!(document(&json!({"type":"doc","attrs":{"onload":"x"}})).is_err());
        assert!(document(&json!({"type":"doc","content":[{"type":"text","text":"Invalid root inline content"}]})).is_err());
        assert!(
            document(&json!({"type":"doc","content":[{"type":"bulletList","content":[]}]}))
                .is_err()
        );
        assert!(document(&json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"heading","attrs":{"level":2}}]}]})).is_err());
    }
}
