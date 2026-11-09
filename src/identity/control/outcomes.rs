//! Settling an unconfirmed write (schema 107): one of the agent's managers or an
//! admin says whether it happened, which releases the hold on the account's writes.
use super::*;

pub(super) fn routes() -> Router<Shared> {
    Router::new().route(
        "/api/identity/workspaces/{w}/control/actions/{id}/outcome",
        post(settle),
    )
}

/// `{"happened": bool, "note": string}`: what the person found in the app.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settle {
    pub happened: bool,
    #[serde(default)]
    pub note: String,
}
impl Settle {
    /// The note as saved: trimmed, at most 500 characters, line breaks allowed.
    pub(crate) fn note(&self) -> Result<Option<String>> {
        let note = self.note.trim();
        ensure!(
            note.chars().count() <= 500,
            "Keep the note to 500 characters"
        );
        ensure!(
            !note.chars().any(|c| c.is_control() && c != '\n'),
            "The note can't contain control characters"
        );
        Ok((!note.is_empty()).then(|| note.to_owned()))
    }
}

/// Someone already gave the opposite answer (409).
#[derive(Debug)]
pub(crate) struct AnswerConflict;
impl std::fmt::Display for AnswerConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Someone already answered differently. Reload to see the saved answer.")
    }
}
impl std::error::Error for AnswerConflict {}

async fn settle(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Settle>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.control_settle(&user, &w, &id, &input).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notes_are_bounded_single_text() {
        let note = |text: &str| {
            Settle {
                happened: true,
                note: text.into(),
            }
            .note()
        };
        assert_eq!(note("  ").unwrap(), None);
        assert_eq!(
            note(" In Sent\nat 10:02 ").unwrap().as_deref(),
            Some("In Sent\nat 10:02")
        );
        assert!(note(&"é".repeat(500)).is_ok());
        assert!(note(&"a".repeat(501)).is_err());
        assert!(note("bell\u{7}").is_err());
    }
}
