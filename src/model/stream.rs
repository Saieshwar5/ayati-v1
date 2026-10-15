use super::{FunctionCall, Message, ToolCall};
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

const MAX_RESPONSE: usize = 256 * 1024;

#[derive(Default)]
pub struct Accumulator {
    pub text: String,
    reasoning: String,
    calls: BTreeMap<usize, ToolCall>,
    finish: Option<String>,
    bytes: usize,
}

impl Accumulator {
    pub fn push(&mut self, data: &str) -> Result<Option<String>> {
        if data == "[DONE]" {
            return Ok(None);
        }
        let value: Value = serde_json::from_str(data).context("Invalid model stream JSON")?;
        ensure!(
            value.get("error").is_none(),
            "Model returned a stream error"
        );
        let Some(choice) = value.get("choices").and_then(|v| v.get(0)) else {
            return Ok(None);
        };
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.finish = Some(reason.into());
        }
        let delta = &choice["delta"];
        let text = delta["content"].as_str().unwrap_or_default();
        self.text.push_str(text);
        self.reasoning
            .push_str(delta["reasoning_content"].as_str().unwrap_or_default());
        if let Some(calls) = delta["tool_calls"].as_array() {
            for call in calls {
                let index = call["index"].as_u64().context("Missing tool call index")? as usize;
                ensure!(index < 16, "Too many model tool calls");
                let entry = self.calls.entry(index).or_insert_with(|| ToolCall {
                    id: String::new(),
                    kind: "function".into(),
                    function: FunctionCall {
                        name: String::new(),
                        arguments: String::new(),
                    },
                });
                entry.id.push_str(call["id"].as_str().unwrap_or_default());
                if let Some(kind) = call["type"].as_str() {
                    entry.kind = kind.into();
                }
                entry
                    .function
                    .name
                    .push_str(call["function"]["name"].as_str().unwrap_or_default());
                entry
                    .function
                    .arguments
                    .push_str(call["function"]["arguments"].as_str().unwrap_or_default());
            }
        }
        self.bytes += data.len();
        ensure!(
            self.bytes <= MAX_RESPONSE,
            "Model response exceeded the stream size limit"
        );
        Ok((!text.is_empty()).then(|| text.into()))
    }

    pub fn finish(self) -> Result<Message> {
        let reason = self
            .finish
            .context("Model stream ended before a complete response")?;
        ensure!(
            matches!(reason.as_str(), "stop" | "tool_calls"),
            "Model response was incomplete or blocked ({reason}); no tool calls were executed"
        );
        let calls: Vec<_> = self.calls.into_values().collect();
        ensure!(
            calls.is_empty() == (reason == "stop"),
            "Inconsistent model finish reason"
        );
        let mut ids = HashSet::new();
        for call in &calls {
            ensure!(
                !call.id.is_empty() && ids.insert(&call.id),
                "Missing or duplicate tool call ID"
            );
            ensure!(
                call.kind == "function" && !call.function.name.is_empty(),
                "Invalid tool call"
            );
        }
        if calls.is_empty() && self.text.trim().is_empty() {
            bail!("Model returned an empty response");
        }
        Ok(Message {
            role: "assistant".into(),
            content: Some(self.text),
            reasoning_content: (!self.reasoning.is_empty()).then_some(self.reasoning),
            tool_calls: calls,
            tool_call_id: None,
        })
    }
}

// Buffer bytes until a full SSE event exists; a UTF-8 character may span network chunks.
#[derive(Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
}
impl SseDecoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        self.buffer.extend_from_slice(bytes);
        ensure!(
            self.buffer.len() <= MAX_RESPONSE,
            "Model stream event is too large"
        );
        let mut events = vec![];
        loop {
            let boundary = self
                .buffer
                .windows(2)
                .position(|w| w == b"\n\n")
                .map(|p| (p, 2))
                .or_else(|| {
                    self.buffer
                        .windows(4)
                        .position(|w| w == b"\r\n\r\n")
                        .map(|p| (p, 4))
                });
            let Some((end, width)) = boundary else { break };
            let block = String::from_utf8(self.buffer.drain(..end + width).collect())
                .context("Model stream is not UTF-8")?;
            let data = block
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(|line| line.strip_prefix(' ').unwrap_or(line))
                .collect::<Vec<_>>()
                .join("\n");
            if !data.is_empty() {
                events.push(data);
            }
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_split_utf8_and_refuses_partial_or_truncated_calls() {
        let mut decoder = SseDecoder::default();
        let event = "data: {\"choices\":[{\"delta\":{\"content\":\"hé\"}}]}\r\n\r\n";
        let split = event.find('é').unwrap() + 1;
        assert!(decoder.feed(&event.as_bytes()[..split]).unwrap().is_empty());
        let mut accumulated = Accumulator::default();
        for data in decoder.feed(&event.as_bytes()[split..]).unwrap() {
            accumulated.push(&data).unwrap();
        }
        assert!(accumulated.finish().is_err());
        let mut truncated = Accumulator::default();
        truncated
            .push(r#"{"choices":[{"delta":{},"finish_reason":"length"}]}"#)
            .unwrap();
        assert!(truncated.finish().is_err());
    }
}
