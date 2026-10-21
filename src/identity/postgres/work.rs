//! Transactional, internal work records. No external side effects or provider credentials.
use super::*;
use crate::identity::chat::store::Job;
use serde_json::json;
impl PgStore {
    pub async fn company_work(&self, user: &User, w: &str) -> Result<Vec<Value>> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let result=json_rows(&tx,"SELECT to_jsonb(t) AS data FROM relay_company_work t JOIN relay_chats c ON c.workspace=t.workspace AND c.id=t.chat WHERE t.workspace=$1 AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=t.workspace AND p.chat=t.chat AND p.user_id=$2)) ORDER BY t.created DESC,t.id LIMIT 200",vec![w.into(),user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn work_grant(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        agent: &str,
        allowed: bool,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Owner",
            "Only the owner can grant work operations"
        );
        super::chat::access(&tx, user, w, chat).await?;
        ensure!(
            query(
                &tx,
                "SELECT agent FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3",
                vec![w.into(), chat.into(), agent.into()]
            )
            .await?
            .is_some(),
            "Agent is not in this conversation"
        );
        if allowed {
            execute(&tx,"INSERT INTO relay_work_grants(workspace,chat,agent) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",vec![w.into(),chat.into(),agent.into()]).await?;
        } else {
            execute(
                &tx,
                "DELETE FROM relay_work_grants WHERE workspace=$1 AND chat=$2 AND agent=$3",
                vec![w.into(), chat.into(), agent.into()],
            )
            .await?;
        }
        audit(
            &tx,
            &user.id,
            Some(w),
            if allowed {
                "work_granted"
            } else {
                "work_revoked"
            },
            &format!("{chat}/{agent}"),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn work_update(
        &self,
        user: &User,
        w: &str,
        id: &str,
        status: &str,
        assignee: &str,
        revision: i64,
    ) -> Result<()> {
        ensure!(
            ["draft", "approved", "rejected", "completed"].contains(&status),
            "Invalid work status"
        );
        let tx = self.workspace_tx(w).await?;
        let item = query(
            &tx,
            "SELECT chat,status FROM relay_company_work WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .context("Work record unavailable")?;
        let chat: String = item.try_get("", "chat")?;
        super::chat::access(&tx, user, w, &chat).await?;
        manager(&tx, w, &user.id).await?;
        let target = User {
            id: assignee.into(),
            email: String::new(),
            display_name: None,
        };
        super::chat::access(&tx, &target, w, &chat).await?;
        let previous: String = item.try_get("", "status")?;
        ensure!(
            previous != "completed" && (status != "completed" || previous == "approved"),
            "Completed work is final; approve before completing"
        );
        ensure!(execute(&tx,"UPDATE relay_company_work SET status=$3,assignee=$4,revision=revision+1 WHERE workspace=$1 AND id=$2 AND revision=$5",vec![w.into(),id.into(),status.into(),assignee.into(),revision.into()]).await?==1,"Work changed; reload before saving");
        audit(
            &tx,
            &user.id,
            Some(w),
            "work_updated",
            &format!("{id}/{status}/{assignee}"),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    /// The caller must select this adapter only in simulation mode. Business writes
    /// still validate the live claim, requester, channel, grant and exact approval.
    pub(crate) async fn scripted_work(&self, job: &Job) -> Result<String> {
        let tx = self.workspace_tx(&job.workspace).await?;
        let w = &job.workspace;
        let requester = User {
            id: job.requester.clone(),
            email: String::new(),
            display_name: None,
        };
        super::chat::access(&tx, &requester, w, &job.chat).await?;
        ensure!(
            super::chat::agent_access(&tx, &job.requester, w, &job.agent).await?,
            "Agent access revoked"
        );
        ensure!(query(&tx,"SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2 AND claim=$3 AND state='running' AND lease_until>$4",vec![w.clone().into(),job.reply.clone().into(),job.claim.clone().into(),now().into()]).await?.is_some(),"Worker claim expired");
        ensure!(
            query(
                &tx,
                "SELECT agent FROM relay_work_grants WHERE workspace=$1 AND chat=$2 AND agent=$3",
                vec![
                    w.clone().into(),
                    job.chat.clone().into(),
                    job.agent.clone().into()
                ]
            )
            .await?
            .is_some(),
            "Agent has no work-operation grant in this channel"
        );
        if let Some(row) = query(
            &tx,
            "SELECT result FROM relay_work_receipts WHERE workspace=$1 AND job=$2",
            vec![w.clone().into(), job.reply.clone().into()],
        )
        .await?
        {
            return Ok(row.try_get("", "result")?);
        }
        let last = job.prompt.lines().last().unwrap_or("");
        let result = if let Some(id) = last
            .split("[complete:")
            .nth(1)
            .and_then(|s| s.split(']').next())
        {
            ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid work ID");
            ensure!(execute(&tx,"UPDATE relay_company_work SET status='completed',revision=revision+1 WHERE workspace=$1 AND chat=$2 AND id=$3 AND status='approved'",vec![w.clone().into(),job.chat.clone().into(),id.into()]).await?==1,"Work needs approval before completion");
            format!("Completed approved work record {id}. No external service was contacted.")
        } else {
            ensure!(
                last.contains("[quote]"),
                "Unsupported scripted work operation"
            );
            let details = json!({"quantity":20,"unit_price_minor":400000,"total_minor":8000000,"currency":"INR","source":"deterministic scenario fixture","tax":"not included"});
            execute(&tx,"INSERT INTO relay_company_work(workspace,chat,id,creator,title,details,assignee,created) VALUES($1,$2,$3,$4,$5,$6,$4,$7)",vec![w.clone().into(),job.chat.clone().into(),job.reply.clone().into(),job.requester.clone().into(),"Quotation: 20 office desks".into(),details.into(),now().into()]).await?;
            format!(
                "Prepared quotation {}: 20 desks × ₹4,000 = ₹80,000 before tax. Saved as draft in Company. A manager must approve this exact record before completion.",
                job.reply
            )
        };
        execute(
            &tx,
            "INSERT INTO relay_work_receipts(workspace,job,result) VALUES($1,$2,$3)",
            vec![
                w.clone().into(),
                job.reply.clone().into(),
                result.clone().into(),
            ],
        )
        .await?;
        audit(
            &tx,
            &job.requester,
            Some(w),
            "agent_work_committed",
            &job.reply,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }
}
