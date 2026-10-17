use super::Platform;
use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};

// Form events must run for ordinary JavaScript forms. Native submissions remain
// blocked by form-action, and the document still has an opaque origin.
pub const CSP: &str = "sandbox allow-scripts allow-forms; default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'; frame-src 'none'; object-src 'none'; form-action 'none'; base-uri 'none'; frame-ancestors 'self'";
impl Platform {
    pub fn app_document(&self, id: &str, document: &str) -> Result<String> {
        let db = self.database()?;
        let version:i64=db.query_row("SELECT s.revision FROM app_sessions s JOIN apps a ON a.id=s.app_id WHERE s.document_id=?1 AND s.app_id=?2 AND s.expires_ms>?3 AND a.state='saved' AND (s.preview=1 OR s.revision=a.active_revision)",params![document,id,crate::storage::intake::now_ms()],|r|r.get(0)).optional()?.context("App document expired or is unavailable")?;
        let app = Self::app_version_in(&db, id, version)?;
        // The bootstrap runs before generated code. It transfers only a scoped
        // MessagePort; the record-session capability stays in the trusted parent.
        Ok(format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><script data-document=\"{document}\">{runtime}</script><style>{css}</style></head><body>{html}<script>{javascript}</script></body></html>",
            runtime = include_str!("../../web/src/app-runtime.js"),
            css = app.css,
            html = app.html,
            javascript = app.javascript
        ))
    }
}
