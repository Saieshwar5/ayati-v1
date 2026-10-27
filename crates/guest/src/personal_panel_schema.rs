use serde_json::{Value, json};
pub fn tool() -> Value {
    json!({
        "name":"save_home_panel",
        "description":format!("Save a personal Home draft for the original human. Use a stable canonical UUID id and expected_revision=0 for a new panel, current revision for a change. {} Sources are completed operation IDs from this task, or [] for findings; observed sources are added automatically. Every revision needs the original person's exact Keep on Home review. You cannot publish, enable maintenance, select another owner/audience or change placement. A saved draft grants no source or business authority.", crate::panel_schema::GUIDE),
        "inputSchema":{
            "type":"object",
            "properties":{
                "id":{"type":"string"},
                "expected_revision":{"type":"integer","minimum":0},
                "title":{"type":"string","maxLength":100},
                "purpose":{"type":"string","maxLength":1000},
                "template":{"type":"string","enum":crate::panel_schema::TEMPLATES},
                "sections":{"type":"array","minItems":1,"maxItems":1,"items":{"type":"object"}},
                "sources":{"type":"array","maxItems":16,"items":{"type":"string"}}
            },
            "required":["id","expected_revision","title","purpose","sections","sources"],
            "additionalProperties":false
        }
    })
}
