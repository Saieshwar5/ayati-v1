use super::*;
use crate::identity::chat::{model::Job, organization_model::Lifecycle};
use serde_json::json;
pub(super) async fn accepting(tx: &impl ConnectionTrait, w: &str, chat: &str) -> Result<()> {
    let row = query(
        tx,
        "SELECT archived FROM relay_chats WHERE workspace=$1 AND id=$2",
        vec![w.into(), chat.into()],
    )
    .await?
    .context("Conversation unavailable")?;
    ensure!(
        !row.try_get::<bool>("", "archived")?,
        "This channel is archived. Restore it before sending new messages."
    );
    Ok(())
}
impl PgStore {
    pub(crate) async fn check_chat_accepting(
        &self,
        user: &User,
        w: &str,
        chat: &str,
    ) -> Result<()> {
        let tx = self.read_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        accepting(&tx, w, chat).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn channel_lifecycle(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        input: Option<&Lifecycle>,
    ) -> Result<Value> {
        if let Some(input) = input {
            input.validate()?;
        }
        let tx = if input.is_some() {
            self.workspace_tx(w).await?
        } else {
            self.read_tx(w).await?
        };
        super::chat::access(&tx, user, w, chat).await?;
        let row=query(&tx,"SELECT kind,creator,archived,archive_revision,archive_request,archive_payload FROM relay_chats WHERE workspace=$1 AND id=$2",vec![w.into(),chat.into()]).await?.context("Conversation unavailable")?;
        let manageable = row.try_get::<String>("", "kind")? != "direct"
            && (role(&tx, w, &user.id).await? == "Owner"
                || row.try_get::<String>("", "creator")? == user.id);
        let mut archived: bool = row.try_get("", "archived")?;
        let mut revision: i64 = row.try_get("", "archive_revision")?;
        if let Some(input) = input {
            ensure!(
                manageable,
                "Only a channel creator or workspace Owner with source access can archive or restore it"
            );
            let payload = serde_json::to_value(input)?;
            if row
                .try_get::<Option<String>>("", "archive_request")?
                .as_deref()
                == Some(&input.request)
            {
                ensure!(
                    row.try_get::<Option<Value>>("", "archive_payload")? == Some(payload),
                    "Channel request payload changed"
                );
            } else {
                ensure!(
                    revision == input.revision,
                    "Channel state changed. Refresh before saving again."
                );
                ensure!(
                    archived != input.archived,
                    "Channel already has this state. Refresh its details."
                );
                execute(&tx,"UPDATE relay_chats SET archived=$3,archive_revision=archive_revision+1,archived_at=CASE WHEN $3 THEN $4 ELSE NULL END,archived_by=CASE WHEN $3 THEN $5 ELSE NULL END,archive_request=$6,archive_payload=$7 WHERE workspace=$1 AND id=$2",vec![w.into(),chat.into(),input.archived.into(),now().into(),user.id.clone().into(),input.request.clone().into(),payload.into()]).await?;
                if input.archived {
                    super::automation_access::invalidate_inaccessible(&tx, w).await?;
                    super::event_dispatch::invalidate_inaccessible(&tx, w).await?;
                }
                archived = input.archived;
                revision += 1;
                audit(
                    &tx,
                    &user.id,
                    Some(w),
                    if archived {
                        "channel_archived"
                    } else {
                        "channel_restored"
                    },
                    chat,
                )
                .await?;
            }
        }
        let mut active = 0;
        for row in rows(&tx,"SELECT j.payload FROM relay_chat_jobs j WHERE j.workspace=$1 AND j.payload->>'chat'=$2 AND j.state<>'finished'",vec![w.into(),chat.into()]).await? {
            let mut job:Job=serde_json::from_value(row.try_get("","payload")?)?;
            job.requester=user.id.clone();
            if super::source_access::allowed(&tx,&job).await? { active+=1; }
        }
        tx.commit().await?;
        Ok(
            json!({"archived":archived,"revision":revision,"manageable":manageable,"active_requests":active}),
        )
    }
}
