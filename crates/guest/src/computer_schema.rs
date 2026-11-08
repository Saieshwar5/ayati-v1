//! Agent computer tools: the controller runs them on the agent's own Linux computer,
//! each request in its own folder. The provider is never named to the agent.
use serde_json::{Value, json};

/// How long the controller may take to answer a tool call, in seconds. Computer and
/// browser work can outlast ordinary calls; the executor's relay to the controller
/// uses the same limits (`control::broker::tool_seconds`).
pub(crate) fn seconds(name: &str, input: &Value) -> u64 {
    match name {
        "computer_run" => input["seconds"].as_u64().unwrap_or(60).clamp(1, 600) + 30,
        _ if name.starts_with("computer_") || name.starts_with("browser_") => 90,
        _ => 45,
    }
}

pub(crate) fn tools() -> Vec<Value> {
    vec![
        json!({"name":"computer_run","description":"Run a shell command on your own Linux computer, in this request's folder. Use it for files and programs: convert or read documents (pdftotext, python3 with openpyxl), work with data, run scripts. Your computer keeps its files between requests; put files every request needs in shared/. It may only reach the websites your owner allowed. Output is shortened; write long results to a file. The command stops after `seconds` (default 60, at most 600), or sooner when this request's time is nearly up; split long work into steps that save to files.","inputSchema":{"type":"object","properties":{"command":{"type":"string","maxLength":4000},"seconds":{"type":"integer","minimum":1,"maximum":600}},"required":["command"],"additionalProperties":false}}),
        json!({"name":"computer_list_files","description":"List files on your computer: this request's folder by default, a path inside it, or shared/ for files kept for every request.","inputSchema":{"type":"object","properties":{"path":{"type":"string","maxLength":500}},"additionalProperties":false}}),
        json!({"name":"computer_read_file","description":"Read a text file from your computer (up to 30,000 characters). Paths are inside this request's folder, or shared/. For PDFs and spreadsheets use computer_run with a converter.","inputSchema":{"type":"object","properties":{"path":{"type":"string","maxLength":500}},"required":["path"],"additionalProperties":false}}),
        json!({"name":"computer_write_file","description":"Save a text file (up to 1 MB) on your computer, in this request's folder or in shared/. It replaces a file with the same name.","inputSchema":{"type":"object","properties":{"path":{"type":"string","maxLength":500},"content":{"type":"string","maxLength":1048576}},"required":["path","content"],"additionalProperties":false}}),
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
    match name {
        "computer_run" => {
            only(&["command", "seconds"])
                && text("command", 4000)
                && v.get("seconds")
                    .is_none_or(|s| s.as_u64().is_some_and(|s| (1..=600).contains(&s)))
        }
        "computer_list_files" => only(&["path"]) && v.get("path").is_none_or(|_| text("path", 500)),
        "computer_read_file" => only(&["path"]) && text("path", 500),
        "computer_write_file" => {
            only(&["path", "content"])
                && text("path", 500)
                && v.get("content")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.len() <= 1_048_576)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn computer_tools_forward_only_their_own_fields() {
        assert!(observed("computer_run", &json!({"command":"ls"})));
        assert!(observed(
            "computer_run",
            &json!({"command":"ls","seconds":600})
        ));
        assert!(!observed(
            "computer_run",
            &json!({"command":"ls","seconds":601})
        ));
        assert!(!observed(
            "computer_run",
            &json!({"command":"ls","cwd":"/"})
        ));
        assert!(observed("computer_list_files", &json!({})));
        assert!(observed("computer_read_file", &json!({"path":"a.txt"})));
        assert!(!observed("computer_read_file", &json!({})));
        assert!(observed(
            "computer_write_file",
            &json!({"path":"a.txt","content":"x"})
        ));
        assert!(!observed("computer_write_file", &json!({"path":"a.txt"})));
        assert_eq!(tools().len(), 4);
        // Long commands and browser work wait longer for the controller than other tools.
        assert_eq!(
            seconds("computer_run", &json!({"command":"ls","seconds":600})),
            630
        );
        assert_eq!(seconds("computer_run", &json!({"command":"ls"})), 90);
        assert_eq!(seconds("browser_open", &json!({"url":"https://x.in"})), 90);
        assert_eq!(seconds("web_search", &json!({"query":"x"})), 45);
    }
}
