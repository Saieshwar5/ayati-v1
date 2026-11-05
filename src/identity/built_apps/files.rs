//! Built apps (Apps AP4–AP7, first part): an app is a small set of files — a manifest,
//! an HTML page and its scripts and styles — kept per version. This module holds what
//! needs no database: the starter every app begins from, the checks a version must pass
//! before it is published, and the self-contained document Cuckoo runs in a sandboxed
//! frame with no network access of its own.
use anyhow::{Result, ensure};
use serde_json::{Value, json};

pub const MAX_FILES: usize = 200;
pub const MAX_FILE: usize = 256 * 1024;
pub const MAX_TOTAL: usize = 2 * 1024 * 1024;

/// The files a new app starts with.
pub fn starter(name: &str, description: &str) -> Vec<(String, String)> {
    let manifest = json!({"name": name, "description": description, "roles": ["user"], "collections": {}, "actions": []});
    vec![
        ("cuckoo-app.json".into(), serde_json::to_string_pretty(&manifest).unwrap_or_default()),
        ("index.html".into(), format!("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<link rel=\"stylesheet\" href=\"style.css\">\n</head>\n<body>\n<main>\n<h1>{}</h1>\n<p>{}</p>\n</main>\n<script src=\"app.js\"></script>\n</body>\n</html>\n", escape(name), escape(name), escape(description))),
        ("style.css".into(), "body { font-family: system-ui, sans-serif; margin: 0; padding: 24px; color: #1f2328; background: #fff; }\n@media (prefers-color-scheme: dark) { body { color: #e6e6e6; background: #17181b; } }\nh1 { font-size: 24px; }\n".into()),
        ("app.js".into(), "// Your app's code. It runs inside Cuckoo, with no network access of its own.\ndocument.querySelector('main').insertAdjacentHTML('beforeend', '<p>Ready.</p>');\n".into()),
        ("README.md".into(), format!("# {name}\n\n{description}\n")),
    ]
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A file path inside an app: plain names and folders, no hidden or parent parts.
pub fn path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= 200
            && path.split('/').all(|part| !part.is_empty()
                && !part.starts_with('.')
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))),
        "Use a file path of letters, numbers, - _ . and /"
    );
    Ok(())
}

/// What a version must pass before it is published, in plain words; empty means ready.
pub fn checks(files: &[(String, String)]) -> Vec<Value> {
    let mut found = vec![];
    let mut fail =
        |check: &str, message: String| found.push(json!({"check": check, "message": message}));
    let get = |name: &str| {
        files
            .iter()
            .find(|(p, _)| p == name)
            .map(|(_, c)| c.as_str())
    };
    match get("cuckoo-app.json").map(serde_json::from_str::<Value>) {
        None => fail("manifest", "cuckoo-app.json is missing".into()),
        Some(Err(_)) => fail("manifest", "cuckoo-app.json is not valid JSON".into()),
        Some(Ok(manifest)) => {
            if manifest["name"].as_str().is_none_or(str::is_empty) {
                fail("manifest", "cuckoo-app.json needs a name".into());
            }
            if !manifest["roles"]
                .as_array()
                .is_some_and(|r| !r.is_empty() && r.iter().all(Value::is_string))
            {
                fail("manifest", "cuckoo-app.json lists at least one role".into());
            }
        }
    }
    if get("index.html").is_none() {
        fail("screens", "index.html is missing".into());
    }
    if files.len() > MAX_FILES {
        fail("size", format!("An app has at most {MAX_FILES} files"));
    }
    if files.iter().map(|(_, c)| c.len()).sum::<usize>() > MAX_TOTAL {
        fail("size", "An app is at most 2 MiB".into());
    }
    for (name, content) in files {
        if let Some(kind) = secret_in(content) {
            fail(
                "secrets",
                format!(
                    "{name} looks like it contains {kind}; keys belong in Cuckoo, never in code"
                ),
            );
        }
        if name.ends_with(".html") || name.ends_with(".js") || name.ends_with(".css") {
            let lower = content.to_lowercase();
            if [
                "src=\"http",
                "src='http",
                "href=\"http",
                "href='http",
                "url(http",
                "import(\"http",
                "import('http",
                "from \"http",
                "from 'http",
                "src=\"//",
                "href=\"//",
            ]
            .iter()
            .any(|p| lower.contains(p))
            {
                fail(
                    "libraries",
                    format!(
                        "{name} loads something from the internet; add the file to the app instead"
                    ),
                );
            }
        }
    }
    found
}

/// Common shapes of keys and tokens that must never be committed.
fn secret_in(content: &str) -> Option<&'static str> {
    if content.contains("-----BEGIN") && content.contains("PRIVATE KEY") {
        return Some("a private key");
    }
    let words = content.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    for word in words {
        if word.len() >= 20
            && word.starts_with("AKIA")
            && word[4..]
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            return Some("a cloud access key");
        }
        if word.len() >= 32
            && (word.starts_with("sk-")
                || word.starts_with("ghp_")
                || word.starts_with("github_pat_")
                || word.starts_with("xoxb-"))
        {
            return Some("an API token");
        }
    }
    None
}

/// The app as one document: its local styles and scripts inlined, and a content policy
/// that blocks every network request, so the sandboxed frame needs nothing from Cuckoo
/// but the messages Cuckoo chooses to answer.
pub fn runnable(files: &[(String, String)]) -> Result<String> {
    let get = |name: &str| {
        files
            .iter()
            .find(|(p, _)| p == name)
            .map(|(_, c)| c.clone())
    };
    let mut html = get("index.html").ok_or_else(|| anyhow::anyhow!("index.html is missing"))?;
    for (name, content) in files {
        if name.ends_with(".css") {
            for quote in ['"', '\''] {
                let tag = format!("<link rel={quote}stylesheet{quote} href={quote}{name}{quote}>");
                html = html.replace(
                    &tag,
                    &format!(
                        "<style>\n{}\n</style>",
                        content.replace("</style", "<\\/style")
                    ),
                );
            }
        }
        if name.ends_with(".js") {
            for quote in ['"', '\''] {
                let tag = format!("<script src={quote}{name}{quote}></script>");
                html = html.replace(
                    &tag,
                    &format!(
                        "<script>\n{}\n</script>",
                        content.replace("</script", "<\\/script")
                    ),
                );
            }
        }
    }
    let policy = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; connect-src 'none'; form-action 'none'; base-uri 'none'\">";
    Ok(match html.find("<head>") {
        Some(at) => format!("{}{policy}{}", &html[..at + 6], &html[at + 6..]),
        None => format!("{policy}{html}"),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_starter_passes_its_checks_and_runs_as_one_blocked_document() {
        let files = super::starter("Leave requests", "Ask for time off");
        assert!(
            super::checks(&files).is_empty(),
            "{:?}",
            super::checks(&files)
        );
        let doc = super::runnable(&files).unwrap();
        assert!(doc.contains("default-src 'none'"));
        assert!(doc.contains("<style>") && doc.contains("document.querySelector('main')"));
        assert!(!doc.contains("src=\"app.js\""));
        assert!(super::path("web/app.js").is_ok());
        assert!(
            super::path("../x").is_err()
                && super::path(".env").is_err()
                && super::path("a//b").is_err()
        );
    }
    #[test]
    fn checks_name_what_blocks_a_release() {
        let mut files = super::starter("A", "B");
        files.retain(|(p, _)| p != "index.html");
        files.push((
            "config.js".into(),
            "const key = 'sk-abcdefghijklmnopqrstuvwxyz0123456789';".into(),
        ));
        files.push((
            "lib.html".into(),
            "<script src=\"https://cdn.example.com/x.js\"></script>".into(),
        ));
        let found: Vec<String> = super::checks(&files)
            .into_iter()
            .map(|f| f["check"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(found, ["screens", "secrets", "libraries"]);
        let mut broken = super::starter("A", "B");
        broken[0].1 = "{".into();
        assert_eq!(
            super::checks(&broken)[0]["message"],
            "cuckoo-app.json is not valid JSON"
        );
    }
}
