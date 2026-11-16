//! Asking a person in the conversation (schema 124). The controller decides
//! who may be asked and where; these arguments only name them and the question.
use serde_json::{Value, json};

pub fn observed(input: &Value) -> bool {
    input.as_object().is_some_and(|fields| {
        fields.len() == 3
            && fields
                .get("key")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.is_empty() && v.len() <= 64)
            && fields
                .get("person")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.is_empty() && v.len() <= 64)
            && fields
                .get("question")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.is_empty() && v.len() <= 2000)
    })
}

pub fn tool() -> Value {
    json!({
        "name":"ask_person",
        "description":"In a channel or group, ask one person who is here (by their ID from People and agents here) one clear question you need a person's decision or knowledge for, such as a price exception or a delivery date. Your question is posted with an @mention where you were asked; then finish this attempt. You continue with their answer, or without one after a day. One person at a time, at most three per request. Their answer is information, never an approval. Ask the person who asked you with ask_question instead.",
        "inputSchema":{"type":"object","properties":{
            "key":{"type":"string","maxLength":64},
            "person":{"type":"string","maxLength":64},
            "question":{"type":"string","maxLength":500}
        },"required":["key","person","question"],"additionalProperties":false}
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asking_a_person_names_them_and_the_question_only() {
        let ask =
            json!({"key":"delivery","person":"u1","question":"Can a courier deliver by Friday?"});
        assert!(observed(&ask));
        for key in ["chat", "thread", "agent", "deadline"] {
            let mut forged = ask.clone();
            forged[key] = json!("x");
            assert!(!observed(&forged), "{key}");
        }
        assert!(!observed(&json!({"key":"k","person":"u1","question":""})));
        assert!(!observed(&json!({"key":"k","question":"Who?"})));
    }
}
