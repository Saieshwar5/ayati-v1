//! JSON-RPC reply extraction for remote MCP servers (plain JSON or SSE).
use anyhow::{Context, Result, ensure};
use serde_json::Value;

/// Find the reply to request `id`; errors never echo the server's message.
pub(crate) fn parse_reply(bytes: &[u8], sse: bool, id: u64) -> Result<Option<Value>> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(None);
    };
    let candidates = if sse {
        let normalized = text.replace("\r\n", "\n");
        normalized
            .split("\n\n")
            .take(normalized.split("\n\n").count().saturating_sub(1))
            .map(|event| {
                event
                    .lines()
                    .filter_map(|l| l.strip_prefix("data:"))
                    .map(|s| s.trim_start())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    } else {
        vec![text.to_owned()]
    };
    for candidate in candidates {
        let Ok(value) = serde_json::from_str::<Value>(&candidate) else {
            continue;
        };
        if value["id"] != id {
            continue;
        }
        ensure!(
            value["jsonrpc"] == "2.0" && value.get("error").is_none(),
            "MCP protocol request failed"
        );
        return Ok(Some(
            value.get("result").context("Missing MCP result")?.clone(),
        ));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn handles_comments_chunked_sse_and_json_without_exposing_errors() {
        let bytes = b": connected\r\n\r\nevent: message\r\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"ok\":true}}\r\n\r\n";
        assert!(
            parse_reply(&bytes[..bytes.len() - 1], true, 2)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            parse_reply(bytes, true, 2).unwrap().unwrap(),
            json!({"ok":true})
        );
        assert!(parse_reply(bytes, true, 3).unwrap().is_none());
        assert!(
            parse_reply(
                br#"{"jsonrpc":"2.0","id":2,"error":{"message":"PRIVATE"}}"#,
                false,
                2
            )
            .unwrap_err()
            .to_string()
            .find("PRIVATE")
            .is_none()
        );
    }
}
