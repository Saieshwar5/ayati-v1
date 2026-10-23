use super::*;
use crate::identity::chat::store::Job;

pub(in crate::identity::postgres) async fn source_allowed(
    db: &impl ConnectionTrait,
    job: &Job,
) -> Result<bool> {
    let user = User {
        id: job.requester.clone(),
        email: String::new(),
        display_name: None,
    };
    for row in rows(db,"SELECT item,revision,access_revision FROM relay_library_sources WHERE workspace=$1 AND reply=$2",vec![job.workspace.clone().into(),job.reply.clone().into()]).await? {
        let id:String=row.try_get("","item")?;
        let Ok(current)=item(db,&user,&job.workspace,&id,false).await else { return Ok(false); };
        if current["access_revision"].as_i64()!=Some(row.try_get("","access_revision")?) { return Ok(false); }
        if query(db,"SELECT agent FROM relay_library_agents WHERE workspace=$1 AND item=$2 AND agent=$3",vec![job.workspace.clone().into(),id.into(),job.agent.clone().into()]).await?.is_none() { return Ok(false); }
        // Source-bearing jobs must remain visible to exactly their requester.
        if query(db,"SELECT id FROM relay_chats c WHERE c.workspace=$1 AND c.id=$2 AND c.visibility='private' AND EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3) AND NOT EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id!=$3)",vec![job.workspace.clone().into(),job.chat.clone().into(),job.requester.clone().into()]).await?.is_none() { return Ok(false); }
    }
    Ok(true)
}
pub(in crate::identity::postgres) async fn source_context(
    db: &impl ConnectionTrait,
    job: &Job,
) -> Result<Vec<Value>> {
    ensure!(
        source_allowed(db, job).await?,
        "Library source access changed"
    );
    json_rows(db,"SELECT jsonb_build_object('id',i.id,'title',coalesce(v.title,i.title),'revision',s.revision,'text',left(coalesce(v.text,i.text),12000),'kind',i.kind) AS data FROM relay_library_sources s JOIN relay_library_items i ON i.workspace=s.workspace AND i.id=s.item LEFT JOIN relay_library_versions v ON v.workspace=s.workspace AND v.item=s.item AND v.revision=s.revision WHERE s.workspace=$1 AND s.reply=$2 ORDER BY i.id LIMIT 3",vec![job.workspace.clone().into(),job.reply.clone().into()]).await
}
impl PgStore {
    pub async fn library_ask(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &AskItem,
    ) -> Result<Value> {
        ensure!(
            !input.question.trim().is_empty() && input.question.len() <= 4000,
            "Write a question up to 4,000 bytes"
        );
        let tx = self.workspace_tx(w).await?;
        let current = item(&tx, user, w, id, false).await?;
        if current["revision"].as_i64() != Some(input.revision) {
            return Err(Conflict.into());
        }
        ensure!(
            current["readiness"] == "ready" || current["readiness"] == "partial",
            "This file has no prepared text for agents"
        );
        ensure!(
            current["text"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
            "Add some content before asking an agent"
        );
        ensure!(query(&tx,"SELECT agent FROM relay_library_agents WHERE workspace=$1 AND item=$2 AND agent=$3",vec![w.into(),id.into(),input.agent.clone().into()]).await?.is_some(),"This agent has not been given access to the item");
        let agent =
            super::super::agent_profiles::managed_access(&tx, &user.id, w, &input.agent, false)
                .await?;
        ensure!(
            agent["scope"] == "workspace"
                && agent["revision"].as_i64().unwrap_or(0) > 0
                && agent["settings"]["paused"] != true,
            "Choose an available workspace agent; private runtime sources are not supported yet"
        );
        let payload = json!({"ask":id,"input":input});
        if let Some(result) = retry(&tx, user, w, &input.request, &payload).await? {
            tx.commit().await?;
            return Ok(result);
        }
        let count:i64=query(&tx,"SELECT count(*) AS n FROM relay_chat_jobs WHERE workspace=$1 AND state IN ('queued','running')",vec![w.into()]).await?.unwrap().try_get("","n")?;
        ensure!(
            count < 32,
            "Workspace agent queue is full. Try again shortly."
        );
        let chat = uuid::Uuid::new_v4().to_string();
        let reply = uuid::Uuid::new_v4().to_string();
        execute(&tx,"INSERT INTO relay_chats(id,workspace,name,creator,kind,description,visibility) VALUES($1,$2,$3,$4,'direct','Private Library question','private')",vec![chat.clone().into(),w.into(),format!("Library · {}",current["title"].as_str().unwrap_or("Source")).into(),user.id.clone().into()]).await?;
        execute(
            &tx,
            "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
            vec![w.into(), chat.clone().into(), user.id.clone().into()],
        )
        .await?;
        execute(
            &tx,
            "INSERT INTO relay_chat_agents(workspace,chat,agent) VALUES($1,$2,$3)",
            vec![w.into(), chat.clone().into(), input.agent.clone().into()],
        )
        .await?;
        let question = format!(
            "{}\n\nSource: [{} · revision {}](/w/{w}/?view=library&item={id})",
            input.question.trim(),
            current["title"]
                .as_str()
                .unwrap_or("Library item")
                .replace(['[', ']', '\n'], ""),
            input.revision
        );
        execute(&tx,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,target) VALUES($1,$2,$3,$4,'user',$5,'sent',$6,$7)",vec![input.request.clone().into(),w.into(),chat.clone().into(),user.id.clone().into(),question.into(),now().into(),input.agent.clone().into()]).await?;
        execute(&tx,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,requester) VALUES($1,$2,$3,$4,'agent','','queued',$5,$6)",vec![reply.clone().into(),w.into(),chat.clone().into(),input.agent.clone().into(),now().into(),user.id.clone().into()]).await?;
        let job = Job {
            reply: reply.clone(),
            chat: chat.clone(),
            workspace: w.into(),
            requester: user.id.clone(),
            agent: input.agent.clone(),
            prompt: input.question.trim().into(),
            claim: None,
        };
        execute(
            &tx,
            "INSERT INTO relay_chat_jobs(workspace,reply,payload,dispatch,run_seconds) VALUES($1,$2,$3,$4,$5)",
            vec![
                w.into(),
                reply.clone().into(),
                serde_json::to_value(job)?.into(),
                if self.pooled {"pooled"} else if self.assigned {"assigned"} else {"polling"}.into(),
                (if self.pooled || self.assigned {self.assignment_seconds as i32} else {180}).into(),
            ],
        )
        .await?;
        execute(
            &tx,
            "INSERT INTO relay_library_sources VALUES($1,$2,$3,$4,$5)",
            vec![
                w.into(),
                reply.into(),
                id.into(),
                input.revision.into(),
                current["access_revision"].as_i64().unwrap().into(),
            ],
        )
        .await?;
        let result = json!({"chat":chat});
        remember(&tx, user, w, &input.request, payload, &result).await?;
        audit(&tx, &user.id, Some(w), "library_agent_requested", id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn library_job_context(&self, job: &Job) -> Result<Vec<Value>> {
        let tx = self.read_tx(&job.workspace).await?;
        ensure!(
            super::super::chat::agent_access(&tx, &job.requester, &job.workspace, &job.agent)
                .await?,
            "Agent access changed"
        );
        let context = source_context(&tx, job).await?;
        tx.commit().await?;
        Ok(context)
    }
}
