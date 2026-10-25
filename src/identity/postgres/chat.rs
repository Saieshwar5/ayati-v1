use super::*;
use crate::identity::chat::store::{Job, NewChat};
use serde_json::json;

pub(super) async fn access(
    db: &impl ConnectionTrait,
    user: &User,
    workspace: &str,
    chat: &str,
) -> Result<()> {
    role(db, workspace, &user.id).await?;
    ensure!(
        query(
            db,
            "SELECT id FROM relay_chats c WHERE workspace=$1 AND id=$2 AND (visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3))",
            vec![workspace.into(), chat.into(), user.id.clone().into()]
        )
        .await?
        .is_some(),
        "Conversation access denied"
    );
    Ok(())
}
pub(super) async fn agent_access(
    db: &impl ConnectionTrait,
    user: &str,
    workspace: &str,
    agent: &str,
) -> Result<bool> {
    let member_role = role(db, workspace, user).await?;
    if let Some(row) = query(
        db,
        "SELECT scope,owner_id,settings FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
        vec![workspace.into(), agent.into()],
    )
    .await?
    {
        let settings: Value = row.try_get("", "settings")?;
        if settings["paused"] == true {
            return Ok(false);
        }
        let owner: Option<String> = row.try_get("", "owner_id")?;
        if row.try_get::<String>("", "scope")? == "private" {
            return Ok(owner.as_deref() == Some(user));
        }
        if owner.as_deref() == Some(user) {
            return Ok(true);
        }
    }
    Ok(member_role == "Owner"
        || query(
            db,
            "SELECT agent FROM relay_chat_access WHERE workspace=$1 AND user_id=$2 AND agent=$3",
            vec![workspace.into(), user.into(), agent.into()],
        )
        .await?
        .is_some())
}
impl PgStore {
    pub async fn agent_access(&self, user: &str, workspace: &str, agent: &str) -> Result<bool> {
        agent_access(&self.db, user, workspace, agent).await
    }
    pub async fn chat_grant(
        &self,
        user: &User,
        workspace: &str,
        target: &str,
        agent: &str,
        allowed: bool,
    ) -> Result<()> {
        let tx = self.workspace_tx(workspace).await?;
        ensure!(
            role(&tx, workspace, &user.id).await? == "Owner",
            "Only the owner can share agents"
        );
        role(&tx, workspace, target).await?;
        ensure!(query(&tx,"SELECT id FROM relay_shared_agents WHERE workspace=$1 AND id=$2 AND scope='private'",vec![workspace.into(),agent.into()]).await?.is_none(),"Private agents cannot be shared");
        if allowed {
            execute(&tx,"INSERT INTO relay_chat_access(workspace,user_id,agent) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",vec![workspace.into(),target.into(),agent.into()]).await?;
        } else {
            execute(
                &tx,
                "DELETE FROM relay_chat_access WHERE workspace=$1 AND user_id=$2 AND agent=$3",
                vec![workspace.into(), target.into(), agent.into()],
            )
            .await?;
            super::task_revocation::inaccessible(&tx, workspace).await?;
        }
        audit(
            &tx,
            &user.id,
            Some(workspace),
            if allowed {
                "agent_chat_granted"
            } else {
                "agent_chat_revoked"
            },
            &format!("{target}/{agent}"),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn directory(&self, user: &User, workspace: &str) -> Result<Value> {
        let tx = self.read_tx(workspace).await?;
        let own_role = role(&tx, workspace, &user.id).await?;
        let users=json_rows(&tx,"SELECT jsonb_build_object('id',u.id,'name',COALESCE(u.display_name,u.email),'email',u.email,'role',m.role) AS data FROM relay_members m JOIN relay_users u ON u.id=m.user_id WHERE m.workspace=$1 ORDER BY m.sequence",vec![workspace.into()]).await?;
        let grants = if own_role == "Owner" {
            json_rows(&tx,"SELECT jsonb_build_object('user',user_id,'agent',agent) AS data FROM relay_chat_access WHERE workspace=$1",vec![workspace.into()]).await?
        } else {
            vec![]
        };
        tx.commit().await?;
        Ok(json!({"users":users,"grants":grants,"role":own_role}))
    }
    pub async fn chat_create(
        &self,
        user: &User,
        workspace: &str,
        input: &NewChat,
    ) -> Result<String> {
        ensure!(
            uuid::Uuid::parse_str(&input.id).is_ok(),
            "Invalid conversation ID"
        );
        ensure!(
            !input.name.trim().is_empty() && input.name.len() <= 100,
            "Use a conversation name of at most 100 characters"
        );
        ensure!(
            input.users.len() <= 100 && input.agents.len() <= 20,
            "Too many participants"
        );
        let direct_key = input.direct_key(&user.id)?;
        let tx = self.workspace_tx(workspace).await?;
        role(&tx, workspace, &user.id).await?;
        for member in &input.users {
            role(&tx, workspace, member).await?;
        }
        for agent in &input.agents {
            let personal=query(&tx,"SELECT owner_id FROM relay_shared_agents WHERE workspace=$1 AND id=$2 AND scope='private'",vec![workspace.into(),agent.clone().into()]).await?;
            if personal.is_some() {
                ensure!(
                    input.kind == "direct" && input.users.is_empty() && input.agents.len() == 1,
                    "Private agents can only have a direct conversation with their owner"
                );
            }
            ensure!(
                agent_access(&tx, &user.id, workspace, agent).await?,
                "Agent is not shared with you"
            );
        }
        if let Some(key) = &direct_key
            && let Some(row) = query(
                &tx,
                "SELECT id FROM relay_chats WHERE workspace=$1 AND direct_key=$2",
                vec![workspace.into(), key.clone().into()],
            )
            .await?
        {
            let id: String = row.try_get("", "id")?;
            access(&tx, user, workspace, &id).await?;
            for member in &input.users {
                access(
                    &tx,
                    &User {
                        id: member.clone(),
                        email: String::new(),
                        display_name: None,
                    },
                    workspace,
                    &id,
                )
                .await?;
            }
            return Ok(id);
        }
        execute(
            &tx,
            "INSERT INTO relay_chats(id,workspace,name,creator,kind,description,direct_key) VALUES($1,$2,$3,$4,$5,$6,$7)",
            vec![
                input.id.clone().into(),
                workspace.into(),
                input.name.trim().into(),
                user.id.clone().into(),
                input.kind.clone().into(),
                input.description.trim().into(),
                direct_key.into(),
            ],
        )
        .await?;
        for member in input.users.iter().chain(std::iter::once(&user.id)) {
            execute(&tx,"INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",vec![workspace.into(),input.id.clone().into(),member.clone().into()]).await?;
        }
        for agent in &input.agents {
            execute(&tx,"INSERT INTO relay_chat_agents(workspace,chat,agent,joined_sequence) VALUES($1,$2,$3,(SELECT COALESCE(max(sequence),0) FROM relay_chat_messages WHERE workspace=$1 AND chat=$2)) ON CONFLICT DO NOTHING",vec![workspace.into(),input.id.clone().into(),agent.clone().into()]).await?;
        }
        tx.commit().await?;
        Ok(input.id.clone())
    }
    pub async fn chats(&self, user: &User, workspace: &str) -> Result<Value> {
        let tx = self.read_tx(workspace).await?;
        role(&tx, workspace, &user.id).await?;
        let chats=json_rows(&tx,"SELECT jsonb_build_object('id',c.id,'name',c.name,'kind',c.kind,'description',c.description,'visibility',c.visibility,'announcements',c.announcements,'users',(SELECT coalesce(jsonb_agg(p2.user_id),'[]'::jsonb) FROM relay_chat_people p2 WHERE p2.workspace=c.workspace AND p2.chat=c.id),'agents',(SELECT coalesce(jsonb_agg(a.agent),'[]'::jsonb) FROM relay_chat_agents a WHERE a.workspace=c.workspace AND a.chat=c.id)) AS data FROM relay_chats c WHERE c.workspace=$1 AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2)) ORDER BY c.sequence DESC",vec![workspace.into(),user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(json!({"chats":chats}))
    }
    pub async fn chat_messages(&self, user: &User, workspace: &str, chat: &str) -> Result<Value> {
        self.chat_page(user, workspace, chat, None).await
    }
    pub async fn chat_job_allowed(&self, job: &Job) -> bool {
        let user = User {
            id: job.requester.clone(),
            email: String::new(),
            display_name: None,
        };
        self.claim_active(job).await.unwrap_or(false)
            && library::source_allowed(&self.db, job)
                .await
                .unwrap_or(false)
            && access(&self.db, &user, &job.workspace, &job.chat)
                .await
                .is_ok()
            && agent_access(&self.db, &user.id, &job.workspace, &job.agent)
                .await
                .unwrap_or(false)
    }
    pub async fn chat_finish(&self, job: &Job, content: &str, state: &str) -> Result<()> {
        self.finish_claimed_job(job, content, state)
            .await
            .map(|_| ())
    }
    pub(crate) async fn finish_claimed_job(
        &self,
        job: &Job,
        content: &str,
        state: &str,
    ) -> Result<bool> {
        let tx = self.workspace_tx(&job.workspace).await?;
        let result = self.finish_job_in(&tx, job, content, state).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub(super) async fn finish_job_in(
        &self,
        tx: &DatabaseTransaction,
        job: &Job,
        content: &str,
        state: &str,
    ) -> Result<bool> {
        ensure!(
            ["complete", "failed", "cancelled"].contains(&state),
            "Invalid reply outcome"
        );
        let Some(claim) = &job.claim else {
            anyhow::bail!("Unclaimed shared job")
        };
        if query(tx,"SELECT reply FROM relay_chat_jobs WHERE reply=$1 AND claim=$2 AND state='running' AND lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint AND workspace=$3 FOR UPDATE",vec![job.reply.clone().into(),claim.clone().into(),job.workspace.clone().into()]).await?.is_none() {
            return Ok(false);
        }
        let user = User {
            id: job.requester.clone(),
            email: String::new(),
            display_name: None,
        };
        let allowed = access(tx, &user, &job.workspace, &job.chat).await.is_ok()
            && library::source_allowed(tx, job).await.unwrap_or(false)
            && agent_access(tx, &user.id, &job.workspace, &job.agent)
                .await
                .unwrap_or(false);
        if allowed
            && state != "cancelled"
            && super::task_continuation::suspend(tx, job, content, state).await?
        {
            return Ok(true);
        }
        execute(tx,"UPDATE relay_chat_jobs SET state='finished',lease_until=0,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND reply=$2",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?;
        execute(tx,"UPDATE relay_worker_assignments SET state='finished',outcome=$2,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state='running'",vec![claim.clone().into(),if allowed { state } else { "cancelled" }.into()]).await?;
        execute(tx,"UPDATE relay_chat_messages SET content=$2,state=$3 WHERE id=$1 AND workspace=$4 AND chat=$5 AND state='queued'",vec![job.reply.clone().into(),if allowed{content}else{"Request cancelled because access changed."}.into(),if allowed{state}else{"cancelled"}.into(),job.workspace.clone().into(),job.chat.clone().into()]).await?;
        Ok(true)
    }
}
