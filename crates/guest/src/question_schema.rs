//! Declarative forwarding schema; the controller validates and owns every record.
use serde_json::{Value, json};
pub(super) fn tool() -> Value {
    let option = json!({"type":"object","properties":{"id":{"type":"string"},"label":{"type":"string"}},"required":["id","label"],"additionalProperties":false});
    let field = json!({"type":"object","properties":{
        "id":{"type":"string"},"label":{"type":"string"},
        "kind":{"type":"string","enum":["text","multiline","number","date","boolean","choice"]},
        "required":{"type":"boolean"},"description":{"type":"string"},
        "options":{"type":"array","maxItems":12,"items":option},
        "suggested":{},"min":{"type":"number"},"max":{"type":"number"},
        "max_length":{"type":"integer","minimum":1,"maximum":4000}
    },"required":["id","label","kind"],"additionalProperties":false});
    json!({"name":"ask_question","description":"Save missing business facts for this task's original requester. Use a stable key for exact retries and new keys only for deliberate follow-ups. Supports single_choice, multiple_choice and flat forms; dates are YYYY-MM-DD. One pending question at a time. The controller binds all identities. After saving, explain what is waiting and finish this attempt without polling or further business operations. An ordinary answer never approves an external action. Do not ask for passwords, tokens or account credentials.",
        "inputSchema":{"type":"object","properties":{"key":{"type":"string","maxLength":64},
          "specification":{"type":"object","properties":{
            "version":{"type":"integer","enum":[1]},"kind":{"type":"string","enum":["single_choice","multiple_choice","form"]},
            "title":{"type":"string","maxLength":240},"description":{"type":"string","maxLength":1000},
            "fallback":{"type":"string","maxLength":1200},"options":{"type":"array","maxItems":12,"items":option},
            "fields":{"type":"array","maxItems":8,"items":field},"allow_other":{"type":"boolean"},
            "min_choices":{"type":"integer","minimum":1},"max_choices":{"type":"integer","minimum":1}
          },"required":["version","kind","title"],"additionalProperties":false}},
          "required":["key","specification"],"additionalProperties":false}})
}
