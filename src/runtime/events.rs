use super::Turn;
use crate::{agent::Event, storage::Store};
use agent_client_protocol::schema::v1::*;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub struct RuntimeEvents {
    pub active: Mutex<Option<Arc<Turn>>>,
    pub store: Arc<Store>,
    pub events: broadcast::Sender<Event>,
    pub proof_tool: bool,
}

impl RuntimeEvents {
    pub fn state(&self, task: &str) {
        let _ = self.events.send(Event {
            kind: "state".into(),
            task_id: task.into(),
            message_id: None,
            text: None,
            offset: None,
        });
    }

    pub fn notification(&self, update: SessionNotification) -> anyhow::Result<()> {
        let turn = self.active.lock().unwrap().clone();
        let Some(turn) =
            turn.filter(|t| t.session == update.session_id && !t.cancel.is_cancelled())
        else {
            return Ok(());
        };
        match update.update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let ContentBlock::Text(text) = chunk.content {
                    let mut output = turn.text.lock().unwrap();
                    anyhow::ensure!(
                        output.len() + text.text.len() <= 1024 * 1024,
                        "Reply exceeded the local output limit"
                    );
                    let offset = output.encode_utf16().count();
                    output.push_str(&text.text);
                    self.store.save_reply(&turn.reply, &output, "streaming")?;
                    let _ = self.events.send(Event {
                        kind: "delta".into(),
                        task_id: turn.task.clone(),
                        message_id: Some(turn.reply.clone()),
                        text: Some(text.text),
                        offset: Some(offset),
                    });
                }
            }
            SessionUpdate::ToolCall(call) => {
                anyhow::ensure!(
                    turn.tools.lock().unwrap().len() < 128,
                    "Too many tool calls in this local run"
                );
                turn.tools
                    .lock()
                    .unwrap()
                    .insert(call.tool_call_id.to_string(), serde_json::to_value(&call)?);
                self.store
                    .set_running_detail(&turn.task, &format!("Tool: {}", call.title))?;
                self.state(&turn.task);
            }
            _ => {} // Do not expose model reasoning or replay session/load chunks as new output.
        }
        Ok(())
    }

    pub async fn permission(&self, request: RequestPermissionRequest) -> RequestPermissionResponse {
        let cancelled = || RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
        let turn = self.active.lock().unwrap().clone();
        let Some(turn) =
            turn.filter(|t| t.session == request.session_id && !t.cancel.is_cancelled())
        else {
            return cancelled();
        };
        let action = match serde_json::to_value(&request.tool_call) {
            Ok(v) => v,
            Err(_) => return cancelled(),
        };
        if action.to_string().len() > 16_000 {
            return cancelled();
        }
        // Goose 1.54 permission updates omit tool metadata. Correlate them with
        // the earlier typed tool-call notification; never authorize by display title.
        let observed = turn
            .tools
            .lock()
            .unwrap()
            .get(&request.tool_call.tool_call_id.to_string())
            .cloned();
        let Some(observed) = observed else {
            return cancelled();
        };
        let tool = observed
            .pointer("/_meta/goose/toolCall/toolName")
            .and_then(|v| v.as_str());
        if !self.proof_tool
            || tool != Some("ayati_proof__get_demo_business")
            || observed.get("rawInput") != action.get("rawInput")
        {
            return cancelled();
        }
        let action = serde_json::json!({ "tool": tool, "request": action });
        let Some(allow) = request
            .options
            .iter()
            .find(|p| p.kind == PermissionOptionKind::AllowOnce)
        else {
            return cancelled();
        };
        let deny = request
            .options
            .iter()
            .find(|p| p.kind == PermissionOptionKind::RejectOnce);
        let title = "Read synthetic demo business data";
        let Ok(id) = self.store.add_permission(&turn.task, title, &action) else {
            return cancelled();
        };
        if self
            .store
            .set_running_detail(&turn.task, "Waiting for your decision")
            .is_err()
        {
            return cancelled();
        }
        self.state(&turn.task);
        let decision = tokio::time::timeout(std::time::Duration::from_secs(300), async {
            loop {
                tokio::select! {
                    biased;
                    _ = turn.cancel.cancelled() => return None,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
                }
                match self.store.permission_decision(&id).as_deref() {
                    Ok("allowed") => return Some(allow.option_id.clone()),
                    Ok("denied") => return deny.map(|p| p.option_id.clone()),
                    Ok("pending") => {}
                    _ => return None,
                }
            }
        })
        .await
        .ok()
        .flatten();
        let _ = self.store.cancel_permissions(&turn.task);
        self.state(&turn.task);
        if turn.cancel.is_cancelled() {
            return cancelled();
        }
        RequestPermissionResponse::new(match decision {
            Some(option) => {
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option))
            }
            None => RequestPermissionOutcome::Cancelled,
        })
    }
}
