//! Browser tools on the agent's own computer: the controller drives Chromium there and
//! returns each page as text with numbered elements. The provider is never named.
use serde_json::{Value, json};

const PAGE: &str = "Returns the page's address, title, text and its numbered elements (links, buttons, fields, menus).";

pub(crate) fn tools() -> Vec<Value> {
    let element = json!({"type":"integer","minimum":1,"maximum":150});
    vec![
        json!({"name":"browser_open","description":format!("Open a website in your computer's browser, for work no app covers. Your computer reaches only the websites your owner allowed. {PAGE}"),"inputSchema":{"type":"object","properties":{"url":{"type":"string","maxLength":2000}},"required":["url"],"additionalProperties":false}}),
        json!({"name":"browser_read","description":format!("Read the page open in your browser again. {PAGE}"),"inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"browser_click","description":format!("Click a numbered element from the last page you read: a link, button, checkbox or tab. Pressing a button that sends something may ask the person first. {PAGE}"),"inputSchema":{"type":"object","properties":{"ref":element},"required":["ref"],"additionalProperties":false}}),
        json!({"name":"browser_type","description":format!("Type into a numbered field, replacing what is there; submit presses Enter afterwards. Never type passwords or one-time codes: use browser_hand_over. {PAGE}"),"inputSchema":{"type":"object","properties":{"ref":element,"text":{"type":"string","maxLength":2000},"submit":{"type":"boolean"}},"required":["ref","text"],"additionalProperties":false}}),
        json!({"name":"browser_choose","description":format!("Choose an option in a numbered menu, by the option's text. {PAGE}"),"inputSchema":{"type":"object","properties":{"ref":element,"option":{"type":"string","maxLength":200}},"required":["ref","option"],"additionalProperties":false}}),
        json!({"name":"browser_press","description":format!("Press a key in the page: Enter, Tab, Escape, Backspace, Space, the arrows, PageUp, PageDown, Home or End. {PAGE}"),"inputSchema":{"type":"object","properties":{"key":{"type":"string","maxLength":20}},"required":["key"],"additionalProperties":false}}),
        json!({"name":"browser_screenshot","description":"Save a picture of the page as a PNG in this request's folder (or shared/), to attach or keep. Use browser_read to understand a page.","inputSchema":{"type":"object","properties":{"name":{"type":"string","maxLength":100}},"additionalProperties":false}}),
        json!({"name":"browser_hand_over","description":"Ask the person who asked to use your browser themselves: to sign in, type a one-time code, prove they're a person or check something. They take over in Cuckoo while you wait; you see nothing meanwhile. Returns a question to ask with ask_question.","inputSchema":{"type":"object","properties":{"need":{"type":"string","enum":["sign_in","code","captcha","check","other"]},"message":{"type":"string","maxLength":500}},"required":["need","message"],"additionalProperties":false}}),
    ]
}

pub(crate) fn observed(name: &str, input: &Value) -> bool {
    let Some(v) = input.as_object() else {
        return false;
    };
    let text = |key: &str, max: usize| {
        v.get(key)
            .and_then(Value::as_str)
            .is_some_and(|s| s.chars().count() <= max)
    };
    let only = |keys: &[&str]| v.keys().all(|k| keys.contains(&k.as_str()));
    let element = || {
        v.get("ref")
            .and_then(Value::as_u64)
            .is_some_and(|r| (1..=150).contains(&r))
    };
    match name {
        "browser_open" => only(&["url"]) && text("url", 2000),
        "browser_read" => v.is_empty(),
        "browser_click" => only(&["ref"]) && element(),
        "browser_type" => {
            only(&["ref", "text", "submit"])
                && element()
                && text("text", 2000)
                && v.get("submit").is_none_or(Value::is_boolean)
        }
        "browser_choose" => only(&["ref", "option"]) && element() && text("option", 200),
        "browser_press" => only(&["key"]) && text("key", 20),
        "browser_screenshot" => only(&["name"]) && v.get("name").is_none_or(|_| text("name", 100)),
        "browser_hand_over" => {
            only(&["need", "message"])
                && v.get("need")
                    .and_then(Value::as_str)
                    .is_some_and(|n| ["sign_in", "code", "captcha", "check", "other"].contains(&n))
                && text("message", 500)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_tools_forward_only_their_own_fields() {
        assert!(observed(
            "browser_open",
            &json!({"url":"https://example.com"})
        ));
        assert!(!observed(
            "browser_open",
            &json!({"url":"https://example.com","x":1})
        ));
        assert!(observed("browser_read", &json!({})));
        assert!(observed("browser_click", &json!({"ref":4})));
        assert!(!observed("browser_click", &json!({"ref":0})));
        assert!(observed(
            "browser_type",
            &json!({"ref":1,"text":"INV-204","submit":true})
        ));
        assert!(!observed(
            "browser_type",
            &json!({"ref":1,"text":"x","submit":"yes"})
        ));
        assert!(observed("browser_choose", &json!({"ref":3,"option":"May"})));
        assert!(observed("browser_press", &json!({"key":"Enter"})));
        assert!(observed("browser_screenshot", &json!({})));
        assert!(observed(
            "browser_hand_over",
            &json!({"need":"sign_in","message":"Sign in to the portal"})
        ));
        assert!(!observed(
            "browser_hand_over",
            &json!({"need":"pay","message":"x"})
        ));
        assert_eq!(tools().len(), 8);
    }
}
