//! Saved shared routines. Model output and viewing never create standing authority.
use super::automation_access::{authority, present, record, visible};
use super::*;
use crate::identity::automations::model::{Change, Schedule, fingerprint, uuid};
use serde_json::json;
impl PgStore {
    pub(crate) fn pooled_execution(&self) -> bool {
        self.pooled
    }
    pub(crate) async fn schedules(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let mut schedules = Vec::new();
        for data in json_rows(&tx,"SELECT to_jsonb(s)-'request_hash' AS data FROM relay_agent_schedules s WHERE workspace=$1 AND agent=$2 ORDER BY created_ms DESC,id DESC LIMIT 100",vec![w.into(),agent.into()]).await? {
            match present(&tx,user,w,agent,data).await {
                Ok(mut data) => {
                    let text = data["content"].as_str().unwrap();
                    let long = text.chars().count() > 240;
                    let preview = text.chars().take(240).collect::<String>();
                    data["content"] = json!(preview);
                    data["instructions_long"] = json!(long);
                    schedules.push(data);
                },
                Err(error) if error.is::<sea_orm::DbErr>()=>return Err(error),
                Err(_)=>{},
            }
        }
        let mut conversations = Vec::new();
        for data in json_rows(&tx,"SELECT jsonb_build_object('id',c.id,'name',c.name,'kind',c.kind) AS data FROM relay_chats c JOIN relay_chat_agents a ON a.workspace=c.workspace AND a.chat=c.id WHERE c.workspace=$1 AND a.agent=$2 AND NOT c.archived AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3)) ORDER BY c.id LIMIT 101",vec![w.into(),agent.into(),user.id.clone().into()]).await? {
            if authority(&tx,user,w,agent,data["id"].as_str().unwrap()).await.is_ok() { conversations.push(data); }
        }
        let more = conversations.len() > 100;
        conversations.truncate(100);
        tx.commit().await?;
        Ok(
            json!({"schedules":schedules,"conversations":conversations,"more_conversations":more,"execution_available":self.pooled}),
        )
    }
    pub(crate) async fn schedule_details(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<Value> {
        uuid(id)?;
        let tx = self.read_tx(w).await?;
        let data = present(&tx, user, w, agent, record(&tx, w, agent, id).await?).await?;
        tx.commit().await?;
        Ok(data)
    }
    pub(crate) async fn save_schedule(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &Schedule,
    ) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        if let Some(old)=query(&tx,"SELECT principal,agent,request_hash FROM relay_agent_schedules WHERE workspace=$1 AND id=$2",vec![w.into(),input.id.clone().into()]).await? {
            ensure!(old.try_get::<String>("","principal")?==user.id && old.try_get::<String>("","agent")?==agent && old.try_get::<String>("","request_hash")?==fingerprint(input)?,"Routine ID belongs to a different saved request");
            let data=present(&tx,user,w,agent,record(&tx,w,agent,&input.id).await?).await?;
            tx.commit().await?; return Ok(data);
        }
        ensure!(
            self.pooled,
            "Shared schedules require pooled background execution"
        );
        authority(&tx, user, w, agent, &input.chat).await?;
        ensure!(
            query(
                &tx,
                "SELECT name FROM pg_timezone_names WHERE name=$1",
                vec![input.timezone.clone().into()]
            )
            .await?
            .is_some(),
            "Choose a supported timezone"
        );
        let now_ms: i64 = query(
            &tx,
            "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS now",
            vec![],
        )
        .await?
        .unwrap()
        .try_get("", "now")?;
        ensure!(input.first_ms > now_ms, "Choose a future first run");
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_agent_schedules WHERE workspace=$1 AND agent=$2",
            vec![w.into(), agent.into()],
        )
        .await?
        .unwrap()
        .try_get("", "n")?;
        ensure!(
            count < 100,
            "This agent has reached the current operational limit of 100 saved routines"
        );
        execute(&tx,"INSERT INTO relay_agent_schedules(workspace,id,agent,principal,chat,name,content,first_ms,interval_seconds,timezone,request_hash,next_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$8)",vec![w.into(),input.id.clone().into(),agent.into(),user.id.clone().into(),input.chat.clone().into(),input.name.trim().into(),input.content.clone().into(),input.first_ms.into(),input.interval_seconds.into(),input.timezone.clone().into(),fingerprint(input)?.into()]).await?;
        audit(&tx, &user.id, Some(w), "agent_schedule_created", &input.id).await?;
        let result = present(&tx, user, w, agent, record(&tx, w, agent, &input.id).await?).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn change_schedule(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        input: &Change,
    ) -> Result<Value> {
        uuid(id)?;
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        let data = record(&tx, w, agent, id).await?;
        visible(&tx, user, w, agent, &data).await?;
        let payload = serde_json::to_value(input)?;
        if let Some(row)=query(&tx,"SELECT actor,payload,result FROM relay_schedule_changes WHERE workspace=$1 AND schedule=$2 AND request=$3",vec![w.into(),id.into(),input.request.clone().into()]).await? {
            ensure!(row.try_get::<String>("","actor")?==user.id && row.try_get::<Value>("","payload")?==payload,"Routine state request changed");
            let result=row.try_get::<Value>("","result")?;tx.commit().await?;return Ok(result);
        }
        ensure!(
            data["revision"] == input.revision,
            "Routine changed. Refresh before saving."
        );
        ensure!(
            data["state"] != "completed",
            "A completed one-time schedule cannot run again"
        );
        if input.enabled {
            ensure!(
                self.pooled,
                "Shared schedules require pooled background execution"
            );
            ensure!(
                data["principal"] == user.id,
                "Only the original authorizing person can resume this routine"
            );
            authority(&tx, user, w, agent, data["chat"].as_str().unwrap()).await?;
        }
        ensure!(
            (data["state"] == "enabled") != input.enabled,
            "Routine already has this state; refresh its details"
        );
        let state = if input.enabled { "enabled" } else { "paused" };
        execute(&tx,"UPDATE relay_agent_schedules SET state=$3,revision=revision+1,detail='' WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),state.into()]).await?;
        if !input.enabled {
            super::automation_dispatch::cancel_unstarted(&tx, w, id).await?;
        }
        let result = json!({"id":id,"state":state,"revision":input.revision+1});
        execute(&tx,"INSERT INTO relay_schedule_changes(workspace,schedule,request,actor,payload,result) VALUES($1,$2,$3,$4,$5,$6)",vec![w.into(),id.into(),input.request.clone().into(),user.id.clone().into(),payload.into(),result.clone().into()]).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            if input.enabled {
                "agent_schedule_resumed"
            } else {
                "agent_schedule_paused"
            },
            id,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn schedule_runs(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<Value> {
        uuid(id)?;
        let tx = self.read_tx(w).await?;
        let data = record(&tx, w, agent, id).await?;
        visible(&tx, user, w, agent, &data).await?;
        let runs=json_rows(&tx,"SELECT jsonb_build_object('due_ms',o.due_ms,'message',o.message,'reply',o.reply,'chat',m.chat,'state',m.state,'progress',m.progress,'job_state',j.state,'created_ms',o.created_ms) AS data FROM relay_schedule_occurrences o JOIN relay_chat_jobs j ON j.workspace=o.workspace AND j.reply=o.reply JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply WHERE o.workspace=$1 AND o.schedule=$2 ORDER BY due_ms DESC LIMIT 20",vec![w.into(),id.into()]).await?;
        tx.commit().await?;
        Ok(json!({"runs":runs}))
    }
}
