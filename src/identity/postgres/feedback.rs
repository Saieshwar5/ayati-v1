use super::*;
use crate::identity::collaboration::Feedback;
impl PgStore {
    pub(crate) async fn feedback(&self, user: &User, w: &str) -> Result<Vec<Value>> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let result=json_rows(&tx,"SELECT to_jsonb(f) AS data FROM relay_feedback f JOIN relay_chats c ON c.workspace=f.workspace AND c.id=f.chat WHERE f.workspace=$1 AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=f.workspace AND p.chat=f.chat AND p.user_id=$2)) ORDER BY f.created DESC,f.id LIMIT 200",vec![w.into(),user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn add_feedback(&self, user: &User, w: &str, input: &Feedback) -> Result<()> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        super::chat::access(&tx, user, w, &input.chat).await?;
        let old = query(
            &tx,
            "SELECT to_jsonb(f) AS data FROM relay_feedback f WHERE workspace=$1 AND id=$2",
            vec![w.into(), input.id.clone().into()],
        )
        .await?;
        if let Some(old) = old {
            let old: Value = old.try_get("", "data")?;
            ensure!(
                old["reporter"] == user.id
                    && old["chat"] == input.chat
                    && old["title"] == input.title
                    && old["expected"] == input.expected
                    && old["actual"] == input.actual
                    && old["category"] == input.category
                    && old["severity"] == input.severity
                    && old["evidence"] == input.evidence,
                "Feedback ID already used"
            );
            return Ok(());
        }
        execute(&tx,"INSERT INTO relay_feedback(workspace,chat,id,reporter,title,expected,actual,category,severity,evidence,created) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",vec![w.into(),input.chat.clone().into(),input.id.clone().into(),user.id.clone().into(),input.title.clone().into(),input.expected.clone().into(),input.actual.clone().into(),input.category.clone().into(),input.severity.clone().into(),input.evidence.clone().into(),now().into()]).await?;
        audit(&tx, &user.id, Some(w), "feedback_created", &input.id).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn feedback_status(
        &self,
        user: &User,
        w: &str,
        id: &str,
        status: &str,
        revision: i64,
    ) -> Result<()> {
        ensure!(
            ["new", "investigating", "fixed", "verified", "deferred"].contains(&status),
            "Invalid feedback status"
        );
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let row = query(
            &tx,
            "SELECT chat FROM relay_feedback WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .context("Feedback missing")?;
        super::chat::access(&tx, user, w, &row.try_get::<String>("", "chat")?).await?;
        ensure!(execute(&tx,"UPDATE relay_feedback SET status=$3,revision=revision+1 WHERE workspace=$1 AND id=$2 AND revision=$4",vec![w.into(),id.into(),status.into(),revision.into()]).await? == 1,"Feedback changed; reload before saving");
        audit(
            &tx,
            &user.id,
            Some(w),
            "feedback_status_updated",
            &format!("{id}/{status}"),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
