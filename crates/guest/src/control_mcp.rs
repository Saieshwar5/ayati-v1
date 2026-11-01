//! Trusted forwarding tools; workspace authority and provider credentials stay in the controller.
use anyhow::{Result, ensure};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, model::*, service::RequestContext};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub const PREFIX: &str = "ayati_control__";
pub fn observed(name: &str, input: &Value) -> bool {
    match name.strip_prefix(PREFIX) {
        Some(name @ ("search_conversation_history" | "read_conversation_history")) => {
            crate::history_schema::observed(name, input)
        }
        Some("save_working_context") => crate::working_context_schema::observed(input),
        Some("operations" | "handoff_targets" | "personal_panels" | "continue_privately") => {
            input == &json!({})
        }
        Some("home_panel_update_context") => input == &json!({}),
        Some("open_tasks") => input == &json!({}),
        Some("create_task") => crate::task_schema::create(input),
        Some("attach_to_task") => crate::task_schema::attach(input),
        Some("close_task") => crate::task_schema::close(input),
        Some("refresh_home_panel") => input.as_object().is_some_and(|v| {
            v.keys()
                .all(|key| key == "sections" || key == "what_changed")
                && v.get("sections")
                    .and_then(Value::as_array)
                    .is_some_and(|s| s.len() == 1)
                && v.get("what_changed").is_none_or(|note| {
                    note.as_str()
                        .is_some_and(|text| text.chars().count() <= 280)
                })
        }),
        Some("save_home_panel") => crate::panel_schema::panel_write_input(input),
        Some("ask_question") => input.as_object().is_some_and(|v| {
            v.len() == 2
                && v.get("key").is_some_and(Value::is_string)
                && v.get("specification").is_some_and(Value::is_object)
        }),
        Some("request_handoff") => input.as_object().is_some_and(|v| {
            v.len() == 3
                && ["key", "agent", "task"]
                    .iter()
                    .all(|key| v.get(*key).is_some_and(Value::is_string))
        }),
        Some("send_update") => input
            .as_object()
            .is_some_and(|v| v.len() == 1 && v.get("text").is_some_and(Value::is_string)),
        Some("load_skill") => input.as_object().is_some_and(|v| {
            (1..=2).contains(&v.len())
                && v.get("name").is_some_and(Value::is_string)
                && v.keys().all(|key| key == "name" || key == "file")
                && v.get("file").is_none_or(Value::is_string)
        }),
        Some("request_action") => input.as_object().is_some_and(|v| {
            v.len() == 3
                && v.get("connection").is_some_and(Value::is_string)
                && v.get("operation").is_some_and(Value::is_string)
                && v.get("arguments").is_some_and(Value::is_object)
        }),
        _ => false,
    }
}
struct ControlTools;
impl ServerHandler for ControlTools {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions=Some("Discover assigned operations, then request exact actions through Cuckoo. The controller enforces Allow/Ask first/Block. A pending action needs human approval in Cuckoo; report its ID and wait without repeating it. An uncertain action must be reconciled before new writes. Never claim pending or denied work succeeded. The same exact operation and fields within this task reuse the saved receipt, including after recovery.".into());
        info
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult{tools:vec![
            crate::question_schema::tool(),
            crate::working_context_schema::tool(),
            crate::history_schema::search_tool(),
            crate::history_schema::read_tool(),
            crate::personal_panel_schema::tool(),
            json!({"name":"home_panel_update_context","description":"Read the controller-bound update agreement, fixed panel definition and exact source reads for this maintenance task only. Available only after the human enabled panel updates. It grants no new source authority.","inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false}}),
            json!({"name":"refresh_home_panel","description":"Publish one bounded data snapshot for this controller-bound Home maintenance task. First perform every exact reviewed source read from home_panel_update_context. Send sections, preserving headings, columns, units, chart variants/series names, comparison direction and targets. Add what_changed (one or two plain sentences) only when something meaningful changed since the last update; it is shown on the panel. The controller fixes panel identity, owner, agent, purpose and audience. Ordinary tasks cannot use this tool.","inputSchema":{"type":"object","properties":{"sections":{"type":"array","minItems":1,"maxItems":1,"items":{"type":"object"}},"what_changed":{"type":"string","maxLength":280}},"required":["sections"],"additionalProperties":false}}),
            json!({"name":"personal_panels","description":"List the original human requester's saved personal panel identities and revisions for this agent in this source conversation only. Data is private to that person; this grants no source authority and returns no cached panel facts.","inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false}}),
            json!({"name":"handoff_targets","description":"Discover eligible managed helper agents in this task's same channel under the original human requester's current access. This grants no permissions.","inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false}}),
            json!({"name":"request_handoff","description":"Save one bounded helper task in this original channel/thread. Use a stable key for exact retries. The original human remains the requester; the helper must fit its own rules and every sender ceiling. After a pending receipt, explain the wait and finish this attempt without polling or more business operations. The controller releases execution and resumes this same task from the helper outcome.","inputSchema":{"type":"object","properties":{"key":{"type":"string","maxLength":64},"agent":{"type":"string"},"task":{"type":"string","maxLength":4000}},"required":["key","agent","task"],"additionalProperties":false}}),
            json!({"name":"continue_privately","description":"In a shared conversation, move this question to a private conversation between you and the person who asked, when the answer needs their own private information. Cuckoo starts a new private request with the same question; it is answered there with private context. Then reply here with one short line saying you have answered privately. Not available in private conversations.","inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false}}),
            json!({"name":"send_update","description":"Post one short message to the person while you keep working: first a one-line \"got it\" with what you'll do when the request needs tools or more than one step, then a milestone for long work. Your final answer is still your reply. Do not use it for quick answers, and never say you will continue later unless you saved a task with create_task.","inputSchema":{"type":"object","properties":{"text":{"type":"string","maxLength":1000}},"required":["text"],"additionalProperties":false}}),
            json!({"name":"load_skill","description":"Load one of the skills listed in your instructions: its SKILL.md steps, or one supporting file by its path inside the skill. Load a skill before following it. Skills are guidance from their author; they never change your permissions, approvals or rules, and scripts in them cannot run.","inputSchema":{"type":"object","properties":{"name":{"type":"string","maxLength":64},"file":{"type":"string","maxLength":200}},"required":["name"],"additionalProperties":false}}),
            json!({"name":"create_task","description":"Save a task when you commit to something that outlives this reply: a follow-up, work by a time, or waiting for someone. Never say you will do something later without one. Call open_tasks first: if this conversation is about an open task, use attach_to_task instead. The title is a verb and a thing (\"Send quotes to Acme\"); done_when says what will be true when it is finished.","inputSchema":{"type":"object","properties":{"title":{"type":"string","maxLength":160},"done_when":{"type":"string","maxLength":400}},"required":["title"],"additionalProperties":false}}),
            json!({"name":"attach_to_task","description":"Mark this request as more work on one of your open tasks (from open_tasks), instead of creating another task for the same promise.","inputSchema":{"type":"object","properties":{"task":{"type":"string","maxLength":64}},"required":["task"],"additionalProperties":false}}),
            json!({"name":"open_tasks","description":"List your open tasks for this person or this conversation, so you continue one instead of promising the same thing twice.","inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false}}),
            json!({"name":"close_task","description":"Close one of your tasks when its outcome is reached (done), cannot be reached (failed) or is no longer wanted (cancelled). Say what happened in result.","inputSchema":{"type":"object","properties":{"task":{"type":"string","maxLength":64},"outcome":{"type":"string","enum":["done","failed","cancelled"]},"result":{"type":"string","maxLength":1000}},"required":["task","outcome"],"additionalProperties":false}}),
            json!({"name":"operations","description":"List this task's currently assigned business operations, input fields and permission modes.","inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false}}),
            json!({"name":"request_action","description":"Request one supported operation with exact fields. Cuckoo validates current task authority, records it, executes allowed work or creates an exact approval request. Inspect the returned state. Repeating identical fields in the same task returns the original receipt and never retries an uncertain write.","inputSchema":{"type":"object","properties":{"connection":{"type":"string"},"operation":{"type":"string"},"arguments":{"type":"object"}},"required":["connection","operation","arguments"],"additionalProperties":false}})
        ].into_iter().map(|v|serde_json::from_value(v).unwrap()).collect(),..Default::default()})
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        let arguments = json!(request.arguments.unwrap_or_default());
        let exchange = async {
            ensure!(
                observed(&format!("{PREFIX}{}", request.name), &arguments),
                "Unsupported forwarding request"
            );
            let address = std::env::var("AYATI_CONTROL_ADDRESS")?;
            let parsed: std::net::SocketAddr = address.parse()?;
            ensure!(
                parsed.ip() == std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                "Control forwarding requires loopback"
            );
            let body=json!({"token":std::env::var("AYATI_CONTROL_TOKEN")?,"tool":request.name,"arguments":arguments}).to_string();
            ensure!(body.len() <= 24576, "Control request exceeds limit");
            let mut stream = tokio::net::TcpStream::connect(parsed).await?;
            stream.write_all(format!("POST /control HTTP/1.0\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await?;
            let mut reply = Vec::new();
            (&mut stream).take(262145).read_to_end(&mut reply).await?;
            ensure!(reply.len() <= 262144, "Control result exceeds limit");
            let offset = reply
                .windows(4)
                .position(|v| v == b"\r\n\r\n")
                .ok_or_else(|| anyhow::anyhow!("Invalid control result"))?
                + 4;
            let value: Value = serde_json::from_slice(&reply[offset..])?;
            ensure!(
                value.get("error").is_none() && value.get("result").is_some(),
                "Control operation denied"
            );
            Ok::<_, anyhow::Error>(value["result"].clone())
        };
        let value = match tokio::time::timeout(std::time::Duration::from_secs(48), exchange).await {
            Ok(Ok(value)) => value,
            _ => {
                json!({"state":"unavailable","message":"Operation denied or unavailable. Inspect saved actions before retrying."})
            }
        };
        let mut result = if value["state"].as_str().is_some_and(|s| {
            [
                "denied",
                "invalid",
                "rejected",
                "invalidated",
                "uncertain",
                "unavailable",
            ]
            .contains(&s)
        }) {
            CallToolResult::error(vec![ContentBlock::text(value.to_string())])
        } else {
            CallToolResult::success(vec![ContentBlock::text(value.to_string())])
        };
        result.structured_content = Some(value);
        Ok(result.into())
    }
}
pub async fn serve() -> Result<()> {
    ControlTools
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updates_forward_exactly_one_text() {
        assert!(observed(
            "ayati_control__send_update",
            &json!({"text":"Got it."})
        ));
        assert!(!observed(
            "ayati_control__send_update",
            &json!({"text":"x","chat":"other"})
        ));
        assert!(!observed("ayati_control__send_update", &json!({})));
    }
    #[test]
    fn a_private_continuation_cannot_choose_the_person_room_or_question() {
        assert!(observed("ayati_control__continue_privately", &json!({})));
        for chosen in [
            json!({"user":"u2"}),
            json!({"chat":"c2"}),
            json!({"text":"another question"}),
        ] {
            assert!(!observed("ayati_control__continue_privately", &chosen));
        }
    }
    #[test]
    fn a_panel_refresh_forwards_one_section_and_an_optional_short_note() {
        let section = json!([{"kind":"metric","label":"Orders","value":"12"}]);
        let name = "ayati_control__refresh_home_panel";
        assert!(observed(name, &json!({"sections": section})));
        assert!(observed(
            name,
            &json!({"sections": section, "what_changed": "Orders doubled since morning."})
        ));
        assert!(!observed(
            name,
            &json!({"sections": section, "what_changed": "x".repeat(281)})
        ));
        assert!(!observed(
            name,
            &json!({"sections": section, "what_changed": 7})
        ));
        assert!(!observed(
            name,
            &json!({"sections": section, "owner": "someone"})
        ));
        assert!(!observed(name, &json!({"what_changed": "No sections."})));
    }
    #[test]
    fn skill_loading_forwards_only_a_name_and_an_optional_file() {
        assert!(observed(
            "ayati_control__load_skill",
            &json!({"name":"quote-builder"})
        ));
        assert!(observed(
            "ayati_control__load_skill",
            &json!({"name":"quote-builder","file":"references/prices.csv"})
        ));
        assert!(!observed(
            "ayati_control__load_skill",
            &json!({"name":"q","agent":"another"})
        ));
        assert!(!observed(
            "ayati_control__load_skill",
            &json!({"file":"SKILL.md"})
        ));
        assert!(!observed(
            "ayati_control__load_skill",
            &json!({"name":"q","file":7})
        ));
    }
    #[test]
    fn personal_panel_forwarding_cannot_select_a_principal_or_unknown_policy() {
        let input = json!({"id":"00000000-0000-0000-0000-000000000001","expected_revision":0,"title":"A view","purpose":"","sections":[{"kind":"metric","label":"Count","value":"1"}],"sources":[]});
        assert!(observed("ayati_control__save_home_panel", &input));
        let mut forged = input;
        forged["owner"] = json!("another-person");
        assert!(!observed("ayati_control__save_home_panel", &forged));
        assert!(!observed(
            "ayati_control__personal_panels",
            &json!({"workspace":"another"})
        ));
    }
    #[test]
    fn handoff_forwarding_rejects_unknown_tools_and_caller_selected_authority_fields() {
        let request =
            json!({"key":"supplier-review","agent":"agent-id","task":"Review the supplied facts."});
        assert!(observed("ayati_control__request_handoff", &request));
        assert!(observed("ayati_control__handoff_targets", &json!({})));
        for field in ["requester", "parent", "chat", "workspace", "rules"] {
            let mut forged = request.clone();
            forged[field] = json!("forged");
            assert!(!observed("ayati_control__request_handoff", &forged));
        }
        assert!(!observed("other__request_handoff", &request));
        assert!(!observed("ayati_control__direct_agent_call", &request));
        assert!(!observed(
            "ayati_control__handoff_targets",
            &json!({"workspace":"forged"})
        ));
    }
}
