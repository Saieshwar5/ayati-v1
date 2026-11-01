use super::*;
use crate::identity::{
    chat::model::{Job, NewMessage},
    communications::model::Message,
};
use serde_json::json;

impl PgStore {
    // Called only after the local simulation route has authenticated its owner.
    pub(crate) async fn simulate_incoming(
        &self,
        actor: &User,
        w: &str,
        id: &str,
        input: &Message,
    ) -> Result<Value> {
        input.validate()?;
        ensure!(
            input.attachments.is_empty(),
            "Simulated external attachments are not supported"
        );
        let tx = self.workspace_tx(w).await?;
        super::access::owner(&tx, actor, w).await?;
        let endpoint = super::endpoints::endpoint(&tx, w, id)
            .await?
            .context("Endpoint unavailable")?;
        ensure!(
            endpoint["simulated"] == true,
            "Only simulated adapters are available"
        );
        let result = self.receive(&tx, w, &endpoint, input).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// One message from a transport whose adapter has already verified the
    /// platform. Shared by every transport: deduplicate, find who may write,
    /// pick the room and admit the request, all in the caller's transaction.
    pub(super) async fn receive(
        &self,
        tx: &DatabaseTransaction,
        w: &str,
        endpoint: &Value,
        input: &Message,
    ) -> Result<Value> {
        ensure!(
            endpoint["address"] == input.recipient,
            "Recipient does not match this endpoint"
        );
        let id = endpoint["id"].as_str().context("Endpoint unavailable")?;
        let fingerprint = input.fingerprint()?;
        if let Some(old)=query(tx,"SELECT fingerprint,status FROM relay_communication_inbox WHERE workspace=$1 AND endpoint=$2 AND event=$3",vec![w.into(),id.into(),input.event.clone().into()]).await? {
            ensure!(old.try_get::<String>("","fingerprint")?==fingerprint,"Event ID belongs to different content");
            return Ok(json!({"accepted":true,"duplicate":true,"status":old.try_get::<String>("","status")?}));
        }
        if endpoint["simulated"] == true {
            let count:i64=query(tx,"SELECT count(*) AS total FROM relay_communication_inbox WHERE workspace=$1 AND transport<>'chat'",vec![w.into()]).await?.unwrap().try_get("","total")?;
            ensure!(count < 1000, "Simulation inbox is full");
        }
        let binding=json_rows(tx,"SELECT to_jsonb(b) AS data FROM relay_communication_bindings b WHERE workspace=$1 AND endpoint=$2 AND sender=$3",vec![w.into(),id.into(),input.sender.clone().into()]).await?.into_iter().next();
        let mut status = "held";
        let mut chat = None;
        let mut message = None;
        let mut job = None;
        if input.automated || endpoint["address"] == input.sender {
            status = "ignored";
        } else if endpoint["enabled"] == true
            && let Some(binding) = binding.filter(|b| b["enabled"] == true)
        {
            let user = User {
                id: binding["user_id"].as_str().unwrap().into(),
                email: String::new(),
                display_name: None,
            };
            let agent = endpoint["agent"].as_str().unwrap();
            // Missing/revoked membership is a held delivery, not a new membership.
            let member = query(
                tx,
                "SELECT user_id FROM relay_members WHERE workspace=$1 AND user_id=$2",
                vec![w.into(), user.id.clone().into()],
            )
            .await?
            .is_some();
            if member && super::chat::agent_access(tx, &user.id, w, agent).await? {
                let c = conversation(tx, w, endpoint, &binding, &input.room()).await?;
                let request = NewMessage {
                    reply_to: None,
                    id: uuid::Uuid::new_v4().to_string(),
                    content: input.text.clone(),
                    agent: Some(agent.into()),
                    thread: None,
                    attachments: vec![],
                };
                job = self.communication_in(tx, &user, w, &c, &request).await?;
                chat = Some(c);
                message = Some(request.id);
                status = "admitted";
            }
        }
        // Held/ignored payloads are not retained. Their digest still fences changed retries.
        let envelope = if status == "admitted" {
            serde_json::to_value(input)?
        } else {
            json!({"event":input.event})
        };
        execute(tx,"INSERT INTO relay_communication_inbox(workspace,endpoint,event,transport,fingerprint,envelope,status,message,reply,chat) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",vec![w.into(),id.into(),input.event.clone().into(),endpoint["transport"].as_str().unwrap().into(),fingerprint.into(),envelope.into(),status.into(),message.into(),job.as_ref().map(|j|j.reply.clone()).into(),chat.into()]).await?;
        Ok(json!({"accepted":true,"duplicate":false,"status":status}))
    }
}

async fn conversation(
    tx: &DatabaseTransaction,
    w: &str,
    e: &Value,
    b: &Value,
    thread: &str,
) -> Result<String> {
    let values = vec![
        w.into(),
        e["id"].as_str().unwrap().into(),
        b["sender"].as_str().unwrap().into(),
        thread.into(),
        e["revision"].as_i64().unwrap().into(),
        b["revision"].as_i64().unwrap().into(),
    ];
    if let Some(row)=query(tx,"SELECT chat FROM relay_communication_conversations WHERE workspace=$1 AND endpoint=$2 AND sender=$3 AND conversation=$4 AND endpoint_revision=$5 AND binding_revision=$6",values.clone()).await? {
        return row.try_get("","chat").map_err(Into::into);
    }
    let chat = uuid::Uuid::new_v4().to_string();
    let (name, description) = match e["server"].as_str() {
        Some(server) if e["simulated"] == false => {
            (server.to_owned(), format!("Messages through {server}"))
        }
        _ => (
            format!("Simulated {}", e["transport"].as_str().unwrap()),
            "Local communication simulation".into(),
        ),
    };
    execute(tx,"INSERT INTO relay_chats(id,workspace,name,creator,kind,visibility,description) VALUES($1,$2,$3,$4,'group','private',$5)",vec![chat.clone().into(),w.into(),name.into(),b["user_id"].as_str().unwrap().into(),description.into()]).await?;
    execute(
        tx,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![
            w.into(),
            chat.clone().into(),
            b["user_id"].as_str().unwrap().into(),
        ],
    )
    .await?;
    execute(
        tx,
        "INSERT INTO relay_chat_agents(workspace,chat,agent,joined_sequence) VALUES($1,$2,$3,0)",
        vec![
            w.into(),
            chat.clone().into(),
            e["agent"].as_str().unwrap().into(),
        ],
    )
    .await?;
    let mut values = values;
    values.push(chat.clone().into());
    execute(tx,"INSERT INTO relay_communication_conversations(workspace,endpoint,sender,conversation,endpoint_revision,binding_revision,chat) VALUES($1,$2,$3,$4,$5,$6,$7)",values).await?;
    Ok(chat)
}

pub(in crate::identity::postgres) async fn record_chat(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    c: &str,
    input: &NewMessage,
    job: Option<&Job>,
) -> Result<()> {
    if !super::access::exists(tx).await? {
        return Ok(());
    }
    let envelope = Message {
        event: input.id.clone(),
        sender: user.id.clone(),
        recipient: c.into(),
        conversation: input.thread.clone().unwrap_or_else(|| c.into()),
        text: input.content.clone(),
        attachments: input.attachments.clone(),
        automated: false,
        thread: None,
    };
    execute(tx,"INSERT INTO relay_communication_inbox(workspace,endpoint,event,transport,fingerprint,envelope,status,message,reply,chat) VALUES($1,$2,$3,'chat',$4,$5,'admitted',$3,$6,$7)",vec![w.into(),format!("chat:{c}").into(),input.id.clone().into(),envelope.fingerprint()?.into(),serde_json::to_value(envelope)?.into(),job.map(|j|j.reply.clone()).into(),c.into()]).await?;
    Ok(())
}

pub(in crate::identity::postgres) async fn context(
    tx: &DatabaseTransaction,
    job: &Job,
) -> Result<Value> {
    if !super::access::exists(tx).await? {
        return Ok(Value::Null);
    }
    Ok(json_rows(tx,"SELECT jsonb_build_object('transport',i.transport,'endpoint',i.endpoint,'event',i.event,'simulated',COALESCE(e.simulated,FALSE)) AS data FROM relay_communication_inbox i LEFT JOIN relay_communication_endpoints e ON e.workspace=i.workspace AND e.id=i.endpoint WHERE i.workspace=$1 AND i.reply=$2",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.into_iter().next().unwrap_or(Value::Null))
}
