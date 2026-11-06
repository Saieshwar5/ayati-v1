//! Answers saved from Chat (LB5): when a person answers a question in a conversation,
//! anyone in it can save that answer as a Library page in a folder they may add to, so
//! agents given the folder answer it themselves next time. The page records the
//! conversation it came from; saving is audited.
use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SaveAnswer {
    pub id: String,
    pub request: String,
    pub folder: Option<String>,
    pub title: String,
    /// The answer, in Markdown.
    pub text: String,
    /// The message it was saved from.
    pub message: String,
}

impl PgStore {
    pub(crate) async fn library_save_answer(
        &self,
        user: &User,
        w: &str,
        input: &SaveAnswer,
    ) -> Result<Value> {
        ensure!(
            !input.text.trim().is_empty() && input.text.len() <= 60_000,
            "Write the answer, up to 60,000 characters"
        );
        {
            let tx = self.read_tx(w).await?;
            let seen = query(&tx, "SELECT 1 AS found FROM relay_chat_messages m WHERE m.workspace=$1 AND m.id=$2 AND (m.author=$3 OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.chat=m.chat AND p.user_id=$3))", vec![w.into(), input.message.clone().into(), user.id.clone().into()]).await?;
            tx.commit().await?;
            ensure!(seen.is_some(), "Save answers from conversations you're in");
        }
        let page = SavePage {
            id: input.id.clone(),
            request: input.request.clone(),
            revision: 0,
            title: input.title.trim().into(),
            document: crate::identity::library::page_text::document(&input.text),
            folder: input.folder.clone(),
        };
        let saved = self.library_save(user, w, &page).await?;
        let tx = self.workspace_tx(w).await?;
        let already = query(&tx, "SELECT 1 AS found FROM relay_audit WHERE workspace=$1 AND action='library_answer_saved' AND target=$2", vec![w.into(), input.id.clone().into()]).await?;
        if already.is_none() {
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "library_answer_saved",
                &input.id,
                &format!("Saved an answer from Chat as \"{}\"", input.title.trim()),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(saved)
    }
}
