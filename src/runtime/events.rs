use super::Turn;
use crate::{agent::Event, storage::Store};
use agent_client_protocol::schema::v1::*;
use std::sync::{Arc, Mutex, atomic::Ordering};
use tokio::sync::broadcast;

pub struct RuntimeEvents {
    pub active: Mutex<Option<Arc<Turn>>>,
    pub store: Arc<Store>,
    pub events: broadcast::Sender<Event>,
    pub proof_tool: bool,
    pub browser: bool,
    pub memory: bool,
}

impl RuntimeEvents {
    pub fn browser_enabled(&self) -> bool {
        self.store
            .profile_edit()
            .map(|p| p.and_then(|p| p.browser).unwrap_or(self.browser))
            .unwrap_or(false)
    }
    pub fn memory_enabled(&self) -> bool {
        self.store
            .profile_edit()
            .map(|p| p.and_then(|p| p.memory).unwrap_or(self.memory))
            .unwrap_or(false)
    }

    pub fn state(&self, task: &str) {
        let _ = self.events.send(Event {
            kind: "state".into(),
            task_id: task.into(),
            message_id: None,
            text: None,
            offset: None,
        });
    }

    pub fn journal_failed(&self, connection_id: &str) {
        if let Some(turn) = self
            .active
            .lock()
            .unwrap()
            .as_ref()
            .filter(|t| t.connection_id == connection_id)
        {
            turn.journal_failed.store(true, Ordering::SeqCst);
        }
    }

    pub fn notification(
        &self,
        connection_id: &str,
        update: SessionNotification,
    ) -> anyhow::Result<()> {
        let turn = self.active.lock().unwrap().clone();
        let Some(turn) = turn.filter(|t| {
            t.connection_id == connection_id
                && t.session == update.session_id
                && !t.cancel.is_cancelled()
                && !t.journal_failed.load(Ordering::SeqCst)
        }) else {
            return Ok(());
        };
        if !self.store.claim_active(&turn.claim)? {
            return Ok(());
        }
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
                    if !self.store.save_claim_reply(
                        &turn.claim,
                        &turn.reply,
                        &output,
                        "streaming",
                    )? {
                        return Ok(());
                    }
                    let _ = self.events.send(Event {
                        kind: "delta".into(),
                        task_id: turn.claim.task_id.clone(),
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
                let observed = serde_json::to_value(&call)?;
                let name = observed
                    .pointer("/_meta/goose/toolCall/toolName")
                    .or_else(|| observed.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                self.store.observe_tool(
                    &turn.claim,
                    &call.tool_call_id.to_string(),
                    name,
                    observed.get("rawInput").unwrap_or(&serde_json::Value::Null),
                )?;
                turn.tools
                    .lock()
                    .unwrap()
                    .insert(call.tool_call_id.to_string(), observed);
                self.store
                    .set_claim_detail(&turn.claim, &format!("Tool: {}", call.title))?;
                self.state(&turn.claim.task_id);
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let value = serde_json::to_value(&update.fields)?;
                if let Some(status) = value.get("status").and_then(|v| v.as_str()) {
                    let status = match status {
                        "in_progress" => "running",
                        "pending" | "completed" | "failed" => status,
                        _ => return Ok(()),
                    };
                    let output = serde_json::json!({"content":value.get("content"),"rawOutput":value.get("rawOutput")});
                    let browser_call = self.browser_enabled()
                        && turn
                            .tools
                            .lock()
                            .unwrap()
                            .get(&update.tool_call_id.to_string())
                            .and_then(|v| v.pointer("/_meta/goose/toolCall/toolName"))
                            .and_then(|v| v.as_str())
                            .is_some_and(|name| {
                                name.starts_with(ayati_guest::browser_policy::PREFIX)
                            });
                    let status = if browser_call
                        && (output.pointer("/rawOutput/response/code")
                            == Some(&serde_json::json!("outcome_unknown"))
                            || output.pointer("/rawOutput/ayatiBrowserOutcome")
                                == Some(&serde_json::json!("uncertain")))
                    {
                        "uncertain"
                    } else {
                        status
                    };
                    self.store.observe_tool_status(
                        &turn.claim,
                        &update.tool_call_id.to_string(),
                        status,
                        Some(&output),
                    )?;
                    self.state(&turn.claim.task_id);
                }
            }
            _ => {} // Do not expose model reasoning or replay session/load chunks as new output.
        }
        Ok(())
    }

    pub async fn permission(
        &self,
        connection_id: &str,
        request: RequestPermissionRequest,
    ) -> RequestPermissionResponse {
        let cancelled = || RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
        let turn = self.active.lock().unwrap().clone();
        let Some(turn) = turn.filter(|t| {
            t.connection_id == connection_id
                && t.session == request.session_id
                && !t.cancel.is_cancelled()
                && !t.journal_failed.load(Ordering::SeqCst)
        }) else {
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
        let browser_allowed = self.browser_enabled()
            && tool.is_some_and(|name| {
                ayati_guest::browser_policy::observed_operation(
                    name,
                    observed.get("rawInput").unwrap_or(&serde_json::Value::Null),
                )
            });
        let proof_allowed = self.proof_tool && tool == Some("ayati_proof__get_demo_business");
        let memory_allowed = self.memory_enabled()
            && tool.is_some_and(|name| {
                ayati_guest::memory_mcp::observed(
                    name,
                    observed.get("rawInput").unwrap_or(&serde_json::Value::Null),
                )
            });
        if !(browser_allowed || proof_allowed || memory_allowed)
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
        let title = if memory_allowed {
            "Use scoped agent memory"
        } else if browser_allowed {
            "Use the agent browser"
        } else {
            "Read synthetic demo business data"
        };
        let Ok(id) = self.store.add_claim_permission(
            &turn.claim,
            &request.tool_call.tool_call_id.to_string(),
            title,
            &action,
        ) else {
            return cancelled();
        };
        if self
            .store
            .set_claim_detail(&turn.claim, "Waiting for your decision")
            .is_err()
        {
            return cancelled();
        }
        self.state(&turn.claim.task_id);
        let decision = tokio::time::timeout(std::time::Duration::from_secs(300), async {
            loop {
                tokio::select! {
                    biased;
                    _ = turn.cancel.cancelled() => return None,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
                }
                match self
                    .store
                    .claim_permission_decision(&turn.claim, &id)
                    .as_deref()
                {
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
        let _ = self.store.cancel_permissions(&turn.claim.task_id);
        self.state(&turn.claim.task_id);
        if turn.cancel.is_cancelled() || !self.store.claim_active(&turn.claim).unwrap_or(false) {
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

#[cfg(test)]
mod tests;
