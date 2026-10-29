use super::*;
use crate::identity::{
    chat::model::{Job, NewMessage},
    handoffs::model::{Request, wait_seconds},
};
use serde_json::json;
async fn participant(db: &impl ConnectionTrait, job: &Job, agent: &str) -> Result<Value> {
    let user = User {
        id: job.requester.clone(),
        email: String::new(),
        display_name: None,
    };
    super::chat::access(db, &user, &job.workspace, &job.chat).await?;
    let profile =
        super::agent_profiles::managed_access(db, &job.requester, &job.workspace, agent, false)
            .await?;
    ensure!(
        profile["scope"] == "workspace" && profile["revision"].as_i64().is_some_and(|r| r > 0),
        "Handoffs require managed workspace agents"
    );
    ensure!(
        super::chat::agent_access(db, &job.requester, &job.workspace, agent).await?,
        "Original requester lacks this agent's request permission"
    );
    ensure!(query(db,"SELECT a.agent FROM relay_chat_agents a JOIN relay_chats c ON c.workspace=a.workspace AND c.id=a.chat WHERE a.workspace=$1 AND a.chat=$2 AND a.agent=$3 AND c.kind='channel' AND (NOT c.announcements OR EXISTS(SELECT 1 FROM relay_members m WHERE m.workspace=$1 AND m.user_id=$4 AND m.role='Owner'))",vec![job.workspace.clone().into(),job.chat.clone().into(),agent.into(),job.requester.clone().into()]).await?.is_some(),"Both agents must participate in the same channel");
    Ok(profile)
}
impl PgStore {
    pub(crate) async fn handoff_targets(&self, job: &Job) -> Result<Value> {
        let tx = self.read_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        ensure!(
            self.pooled,
            "Channel handoffs need pooled background execution"
        );
        participant(&tx, job, &job.agent).await?;
        super::channel_lifecycle::accepting(&tx, &job.workspace, &job.chat).await?;
        let ancestors = super::handoff_access::tree(&tx, job).await?;
        let mut lineage = vec![job.agent.clone()];
        let mut child = job.reply.clone();
        for _ in 0..3 {
            let Some(h) = ancestors.iter().find(|h| h["child"] == child) else {
                break;
            };
            lineage.push(h["sender"].as_str().unwrap().into());
            child = h["parent"].as_str().unwrap().into();
        }
        let mut agents = vec![];
        for candidate in json_rows(&tx,"SELECT jsonb_build_object('id',a.id,'name',a.name,'role',a.role) AS data FROM relay_shared_agents a JOIN relay_chat_agents p ON p.workspace=a.workspace AND p.agent=a.id WHERE a.workspace=$1 AND p.chat=$2 AND a.scope='workspace' AND a.revision>0 ORDER BY a.name,a.id LIMIT 100",vec![job.workspace.clone().into(),job.chat.clone().into()]).await? {
   let id=candidate["id"].as_str().unwrap();if lineage.iter().any(|v|v==id){continue}
   let access=async{participant(&tx,job,id).await?;super::handoff_access::event_source(&tx,job,id).await?;Ok::<_,anyhow::Error>(())}.await;
   match access {Ok(())=>agents.push(candidate),Err(e) if e.is::<sea_orm::DbErr>()=>return Err(e),Err(_)=>{}}
  }
        tx.commit().await?;
        Ok(
            json!({"agents":agents,"message":"Helpers keep their own permissions and the sender's original limits. Use a stable request key; finish this attempt after saving a handoff."}),
        )
    }
    pub(crate) async fn request_handoff(&self, job: &Job, input: &Request) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let hash = crate::identity::events::model::fingerprint(
            &json!({"agent":input.agent,"task":input.task}),
        )?;
        if let Some(row)=query(&tx,"SELECT id,input_hash,state,child,request_key FROM relay_agent_handoffs WHERE workspace=$1 AND parent=$2 AND (request_key=$3 OR input_hash=$4) ORDER BY (request_key=$3) DESC LIMIT 1",vec![job.workspace.clone().into(),job.reply.clone().into(),input.key.clone().into(),hash.clone().into()]).await? {
   ensure!(row.try_get::<String>("","input_hash")?==hash,"Handoff key belongs to different instructions");let result=json!({"id":row.try_get::<String>("","id")?,"key":row.try_get::<String>("","request_key")?,"state":row.try_get::<String>("","state")?,"reply":row.try_get::<String>("","child")?,"message":"Saved handoff receipt. Do not create another request for the same work."});tx.commit().await?;return Ok(result)
  }
        ensure!(
            self.pooled,
            "Channel handoffs need pooled background execution"
        );
        super::handoff_controls::ready_for_new_work(&tx, job).await?;
        ensure!(query(&tx,"SELECT j.reply FROM relay_chat_jobs j WHERE j.workspace=$1 AND j.reply=$2 AND (EXISTS(SELECT 1 FROM relay_task_questions q WHERE q.workspace=j.workspace AND q.reply=j.reply AND q.state='pending') OR EXISTS(SELECT 1 FROM relay_control_actions a WHERE a.workspace=j.workspace AND a.job=j.reply AND a.state IN ('pending','executing','uncertain')))",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.is_none(),"Resolve the current question or operation before delegating another task");
        participant(&tx, job, &job.agent).await?;
        participant(&tx, job, &input.agent).await?;
        super::channel_lifecycle::accepting(&tx, &job.workspace, &job.chat).await?;
        ensure!(
            job.agent != input.agent,
            "An agent cannot hand off to itself"
        );
        let tree = super::handoff_access::tree(&tx, job).await?;
        let parent = tree.iter().find(|h| h["child"] == job.reply);
        let depth = parent.and_then(|h| h["depth"].as_i64()).unwrap_or(0) + 1;
        ensure!(
            depth <= 3
                && tree.len() < 8
                && tree.iter().filter(|h| h["parent"] == job.reply).count() < 2,
            "Handoff depth, total tasks or direct-child limit reached"
        );
        let mut ancestor = job.agent.clone();
        let mut reply = job.reply.clone();
        for _ in 0..4 {
            ensure!(
                ancestor != input.agent,
                "Handoff would create an agent cycle"
            );
            let Some(h) = tree.iter().find(|h| h["child"] == reply) else {
                break;
            };
            ancestor = h["sender"].as_str().unwrap().into();
            reply = h["parent"].as_str().unwrap().into();
        }
        let root = super::handoff_access::root(&tx, job).await?;
        let row=query(&tx,"SELECT j.run_cycle,m.thread FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply WHERE j.workspace=$1 AND j.reply=$2",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.unwrap();
        let cycle: i32 = row.try_get("", "run_cycle")?;
        ensure!(cycle < 19, "Task is at its continuation limit");
        let thread = row
            .try_get::<Option<String>>("", "thread")?
            .unwrap_or_else(|| job.reply.clone());
        let expires = parent
            .and_then(|h| h["expires_ms"].as_i64())
            .or_else(|| tree.first().and_then(|h| h["expires_ms"].as_i64()))
            .unwrap_or(now() * 1000 + wait_seconds()? * 1000);
        ensure!(
            expires > now() * 1000,
            "The root handoff deadline has expired"
        );
        let controls = super::handoff_controls::snapshot(&tx, job).await?;
        let source = super::handoff_access::event_source(&tx, job, &input.agent).await?;
        let request = NewMessage {
            id: uuid::Uuid::new_v4().to_string(),
            agent: Some(input.agent.clone()),
            thread: Some(thread.clone()),
            attachments: vec![],
            content: input.task.clone(),
        };
        let child = self
            .handoff_in(&tx, job, &request)
            .await?
            .context("Handoff task was not accepted")?;
        let id = uuid::Uuid::new_v4().to_string();
        execute(&tx,"INSERT INTO relay_agent_handoffs(workspace,id,parent,root,child,request_message,request_key,input_hash,principal,sender,target,chat,thread,task,controls,source,depth,run_cycle,expires_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)",vec![job.workspace.clone().into(),id.clone().into(),job.reply.clone().into(),root.into(),child.reply.clone().into(),request.id.into(),input.key.clone().into(),hash.into(),job.requester.clone().into(),job.agent.clone().into(),input.agent.clone().into(),job.chat.clone().into(),thread.into(),input.task.clone().into(),controls.into(),source.into(),(depth as i32).into(),cycle.into(),expires.into()]).await?;
        audit(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "agent_channel_handoff",
            &id,
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"id":id,"state":"pending","reply":child.reply,"message":"Helper task saved in the original channel/thread. Explain what is waiting and finish this bounded attempt. The controller releases your worker and resumes the same task when the helper ends; do not poll or request more business actions."}),
        )
    }
}
