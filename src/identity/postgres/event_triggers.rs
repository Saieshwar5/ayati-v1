//! Human-reviewed standing event instructions; receipt and execution authority stay separate.
use super::event_access::{record, versions, visible};
use super::*;
use crate::identity::events::{
    catalog::Source,
    model::{Change, Trigger, fingerprint},
};
use serde_json::json;
async fn choice(
    db: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    source: &Source,
) -> Result<Value> {
    source.secret()?;
    let row=query(db,"SELECT r.chat,c.name,c.visibility,conn.definition->>'account' AS account,conn.definition->>'name' AS account_name FROM relay_control_effective_rules r JOIN relay_chats c ON c.workspace=r.workspace AND c.id=r.chat JOIN relay_control_connections conn ON conn.workspace=r.workspace AND conn.id=r.connection WHERE r.workspace=$1 AND r.agent=$2 AND r.connection=$3 AND r.operation=$4 AND r.mode='allow'",vec![w.into(),agent.into(),source.connection.clone().into(),source.read_operation.clone().into()]).await?.context("Configure an Allow read rule in a destination first")?;
    let chat: String = row.try_get("", "chat")?;
    super::automation_access::authority(db, user, w, agent, &chat).await?;
    let authority = versions(db, user, w, agent, &chat, source).await?;
    Ok(
        json!({"source":source.public(),"chat":chat,"chat_name":row.try_get::<String>("","name")?,"visibility":row.try_get::<String>("","visibility")?,"account":row.try_get::<Option<String>>("","account")?,"account_name":row.try_get::<Option<String>>("","account_name")?,"authority":authority}),
    )
}
async fn present(
    db: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    mut data: Value,
    sources: &[Source],
    full: bool,
) -> Result<Value> {
    visible(db, user, w, agent, &data).await?;
    let names=query(db,"SELECT c.name,COALESCE(u.display_name,u.email) AS person FROM relay_chats c JOIN relay_users u ON u.id=$3 WHERE c.workspace=$1 AND c.id=$2",vec![w.into(),data["chat"].as_str().unwrap().into(),data["principal"].as_str().unwrap().into()]).await?.context("Event destination unavailable")?;
    data["chat_name"] = json!(names.try_get::<String>("", "name")?);
    data["principal_name"] = json!(names.try_get::<String>("", "person")?);
    data["name"] = data["input"]["name"].clone();
    data["filter"] = data["input"]["filter"].clone();
    let text = data["input"]["instructions"].as_str().unwrap();
    let long = !full && text.chars().count() > 240;
    let text = if full {
        text.to_owned()
    } else {
        text.chars().take(240).collect()
    };
    data["instructions"] = json!(text);
    data["instructions_long"] = json!(long);
    data.as_object_mut().unwrap().remove("input");
    let source = sources.iter().find(|s| {
        s.id == data["source"] && s.fingerprint().ok().as_deref() == data["binding"].as_str()
    });
    let current = if let Some(source) = source {
        match choice(db, user, w, agent, source).await {
            Ok(v) => Some(v),
            Err(e) if e.is::<sea_orm::DbErr>() => return Err(e),
            Err(_) => None,
        }
    } else {
        None
    };
    data["can_resume"] = json!(
        data["principal"] == user.id && current.as_ref().is_some_and(|c| c["chat"] == data["chat"])
    );
    data["choice"] = json!(current);
    Ok(data)
}
impl PgStore {
    pub(crate) async fn event_triggers(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        sources: &[Source],
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let mut choices = vec![];
        for source in sources {
            if source.secret().is_ok() {
                match choice(&tx, user, w, agent, source).await {
                    Ok(c) => choices.push(c),
                    Err(e) if e.is::<sea_orm::DbErr>() => return Err(e),
                    Err(_) => {}
                }
            }
        }
        let mut triggers = vec![];
        for data in json_rows(&tx,"SELECT to_jsonb(t)-'request_hash' AS data FROM relay_event_triggers t WHERE workspace=$1 AND agent=$2 ORDER BY created_ms DESC,id DESC LIMIT 100",vec![w.into(),agent.into()]).await? {
            match present(&tx,user,w,agent,data,sources,false).await {Ok(data)=>triggers.push(data),Err(e) if e.is::<sea_orm::DbErr>()=>return Err(e),Err(_)=>{}}
        }
        tx.commit().await?;
        Ok(json!({"triggers":triggers,"choices":choices,"execution_available":self.pooled}))
    }
    pub(crate) async fn event_trigger_details(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        sources: &[Source],
    ) -> Result<Value> {
        crate::identity::automations::model::uuid(id)?;
        let tx = self.read_tx(w).await?;
        let data = present(
            &tx,
            user,
            w,
            agent,
            record(&tx, w, agent, id).await?,
            sources,
            true,
        )
        .await?;
        tx.commit().await?;
        Ok(data)
    }
    pub(crate) async fn save_event_trigger(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &Trigger,
        source: Option<&Source>,
    ) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        if let Some(row)=query(&tx,"SELECT principal,agent,request_hash FROM relay_event_triggers WHERE workspace=$1 AND id=$2",vec![w.into(),input.id.clone().into()]).await? {
            ensure!(row.try_get::<String>("","principal")?==user.id && row.try_get::<String>("","agent")?==agent && row.try_get::<String>("","request_hash")?==fingerprint(input)?,"Event-trigger ID belongs to a different request");
            let data=present(&tx,user,w,agent,record(&tx,w,agent,&input.id).await?,source.map(std::slice::from_ref).unwrap_or_default(),true).await?;tx.commit().await?;return Ok(data);
        }
        let source = source.context("Event source is unavailable")?;
        ensure!(
            self.pooled,
            "Event work requires pooled background execution"
        );
        ensure!(
            source.workspace == w
                && source.id == input.source
                && source.fingerprint()? == input.binding,
            "Source changed; review it again"
        );
        source.secret()?;
        super::automation_access::authority(&tx, user, w, agent, &input.chat).await?;
        ensure!(
            versions(&tx, user, w, agent, &input.chat, source).await?
                == super::event_access::expected(input),
            "Source read/delegation authority changed; refresh before activation"
        );
        ensure!(
            input
                .filter
                .as_object()
                .unwrap()
                .keys()
                .all(|key| source.fields.contains(key)),
            "Filter only the reviewed included fields"
        );
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_event_triggers WHERE workspace=$1 AND agent=$2",
            vec![w.into(), agent.into()],
        )
        .await?
        .unwrap()
        .try_get("", "n")?;
        ensure!(
            count < 100,
            "The current operational limit is 100 saved event triggers per agent"
        );
        execute(&tx,"INSERT INTO relay_event_triggers(workspace,id,agent,principal,source,binding,source_definition,connection,read_operation,chat,input,request_hash,connection_revision,read_revision,delegation_revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",vec![w.into(),input.id.clone().into(),agent.into(),user.id.clone().into(),source.id.clone().into(),input.binding.clone().into(),source.public().into(),source.connection.clone().into(),source.read_operation.clone().into(),input.chat.clone().into(),serde_json::to_value(input)?.into(),fingerprint(input)?.into(),input.connection_revision.into(),input.read_revision.into(),input.delegation_revision.into()]).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            "agent_event_trigger_created",
            &input.id,
        )
        .await?;
        let data = present(
            &tx,
            user,
            w,
            agent,
            record(&tx, w, agent, &input.id).await?,
            std::slice::from_ref(source),
            true,
        )
        .await?;
        tx.commit().await?;
        Ok(data)
    }
    pub(crate) async fn change_event_trigger(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        input: &Change,
        sources: &[Source],
    ) -> Result<Value> {
        input.validate()?;
        crate::identity::automations::model::uuid(id)?;
        let tx = self.workspace_tx(w).await?;
        let data = record(&tx, w, agent, id).await?;
        visible(&tx, user, w, agent, &data).await?;
        let payload = serde_json::to_value(input)?;
        if let Some(row)=query(&tx,"SELECT actor,payload,result FROM relay_event_trigger_changes WHERE workspace=$1 AND trigger_id=$2 AND request=$3",vec![w.into(),id.into(),input.request.clone().into()]).await? {
            ensure!(row.try_get::<String>("","actor")?==user.id && row.try_get::<Value>("","payload")?==payload,"Event-trigger state request changed");let result=row.try_get::<Value>("","result")?;tx.commit().await?;return Ok(result);
        }
        ensure!(
            data["revision"] == input.revision,
            "Event trigger changed; refresh its review"
        );
        ensure!(
            (data["state"] == "enabled") != input.enabled,
            "Event trigger already has this state"
        );
        let authority = if input.enabled {
            ensure!(
                self.pooled && data["principal"] == user.id,
                "Only the original person can resume in pooled mode"
            );
            let source = sources
                .iter()
                .find(|s| s.id == data["source"])
                .context("Event source is unavailable")?;
            ensure!(
                source.fingerprint()? == data["binding"],
                "Source identity or projection changed; create a new trigger"
            );
            source.secret()?;
            super::automation_access::authority(
                &tx,
                user,
                w,
                agent,
                data["chat"].as_str().unwrap(),
            )
            .await?;
            let authority =
                versions(&tx, user, w, agent, data["chat"].as_str().unwrap(), source).await?;
            ensure!(
                authority
                    == json!({"connection_revision":input.connection_revision,"read_revision":input.read_revision,"delegation_revision":input.delegation_revision}),
                "Read authority changed; refresh before resuming"
            );
            authority
        } else {
            json!({"connection_revision":data["connection_revision"],"read_revision":data["read_revision"],"delegation_revision":data["delegation_revision"]})
        };
        let state = if input.enabled { "enabled" } else { "paused" };
        execute(&tx,"UPDATE relay_event_triggers SET state=$3,revision=revision+1,detail='',connection_revision=$4,read_revision=$5,delegation_revision=$6 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),state.into(),authority["connection_revision"].as_i64().into(),authority["read_revision"].as_i64().into(),authority["delegation_revision"].as_i64().into()]).await?;
        if !input.enabled {
            super::event_dispatch::cancel_pending(&tx, w, id).await?;
        }
        let result = json!({"id":id,"state":state,"revision":input.revision+1});
        execute(
            &tx,
            "INSERT INTO relay_event_trigger_changes VALUES($1,$2,$3,$4,$5,$6)",
            vec![
                w.into(),
                id.into(),
                input.request.clone().into(),
                user.id.clone().into(),
                payload.into(),
                result.clone().into(),
            ],
        )
        .await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            if input.enabled {
                "agent_event_trigger_resumed"
            } else {
                "agent_event_trigger_paused"
            },
            id,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn event_runs(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let data = record(&tx, w, agent, id).await?;
        visible(&tx, user, w, agent, &data).await?;
        let runs=json_rows(&tx,"SELECT jsonb_build_object('id',d.id,'state',d.state,'revoked',d.revoked,'detail',d.detail,'received_ms',i.received_ms,'message',d.message,'reply',d.reply,'chat',t.chat,'task_state',m.state,'job_state',j.state,'progress',CASE WHEN d.revoked THEN NULL ELSE jsonb_build_object('state',m.progress->>'state') END) AS data FROM relay_event_deliveries d JOIN relay_event_inbox i ON i.workspace=d.workspace AND i.source=d.source AND i.id=d.event_id JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id LEFT JOIN relay_chat_jobs j ON j.workspace=d.workspace AND j.reply=d.reply LEFT JOIN relay_chat_messages m ON m.workspace=d.workspace AND m.id=d.reply WHERE d.workspace=$1 AND d.trigger_id=$2 ORDER BY i.sequence DESC LIMIT 20",vec![w.into(),id.into()]).await?;
        tx.commit().await?;
        Ok(json!({"runs":runs}))
    }
}
