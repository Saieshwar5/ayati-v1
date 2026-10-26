use serde_json::{Value, json};
pub(super) fn tool() -> Value {
    let entry = json!({"type":"object","properties":{"text":{"type":"string","maxLength":800},"sources":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":8}},"required":["text","sources"],"additionalProperties":false});
    let list = json!({"type":"array","items":entry,"maxItems":8});
    json!({"name":"save_working_context","description":"Save a compact evidence-linked interpretation for this human, agent and conversation. Use the supplied revision and exact message/note IDs from this attempt or retrieved history. Report decisions, unresolved questions and corrections separately. This grants no authority and proves no operation succeeded. External-source or business-action tasks cannot save generic summaries. A conflict requires fresh context; never guess a larger revision.","inputSchema":{"type":"object","properties":{"revision":{"type":"integer","minimum":0},"objective":entry,"decisions":list,"questions":list,"corrections":list},"required":["revision","objective","decisions","questions","corrections"],"additionalProperties":false}})
}
pub(super) fn observed(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        v.len() == 5
            && input["revision"].as_i64().is_some()
            && input["objective"].is_object()
            && ["decisions", "questions", "corrections"]
                .iter()
                .all(|k| input[*k].is_array())
    })
}
