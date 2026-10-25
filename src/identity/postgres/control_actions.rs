//! Exact persisted requests; action admission and external dispatch have separate ownership.
use super::*;
use crate::identity::{
    chat::store::Job,
    control::model::{ActionRequest, Operation},
};
use serde_json::json;

pub(crate) struct Dispatch {
    pub id: String,
    pub token: String,
    pub server: String,
    pub fingerprint: String,
    pub operation: Operation,
    pub arguments: Value,
    pub origin: Option<Job>,
}
pub(crate) struct Proposed {
    pub action: Value,
    pub dispatch: Option<Dispatch>,
}
impl PgStore {
    pub(crate) async fn control_operations(&self, job: &Job) -> Result<Value> {
        let tx = self.read_tx(&job.workspace).await?;
        authorize(&tx, job, true).await?;
        let operations=json_rows(&tx,"SELECT jsonb_build_object('connection',c.id,'operation',op->>'id','title',op->>'title','effect',op->>'effect','schema',op->'schema','mode',r.mode) AS data FROM relay_control_rules r JOIN relay_control_connections c ON c.workspace=r.workspace AND c.id=r.connection CROSS JOIN LATERAL jsonb_array_elements(c.definition->'operations') op WHERE r.workspace=$1 AND r.agent=$2 AND r.chat=$3 AND c.enabled AND op->>'id'=r.operation AND r.mode<>'block' ORDER BY c.id,r.operation",vec![job.workspace.clone().into(),job.agent.clone().into(),job.chat.clone().into()]).await?;
        tx.commit().await?;
        Ok(json!({"operations":operations}))
    }
    pub(crate) async fn control_propose(
        &self,
        job: &Job,
        input: &ActionRequest,
    ) -> Result<Proposed> {
        // Same exact operation within one job has one identity even after a worker
        // retry. The model cannot evade the receipt by generating another UUID.
        let request_id = crate::identity::provider::digest(&serde_json::to_string(input)?);
        let tx = self.workspace_tx(&job.workspace).await?;
        authorize(&tx, job, true).await?;
        if let Some(action)=query(&tx,"SELECT to_jsonb(a) AS data FROM relay_control_actions a WHERE workspace=$1 AND job=$2 AND request_id=$3",vec![job.workspace.clone().into(),job.reply.clone().into(),request_id.clone().into()]).await? {
            let action:Value=action.try_get("","data")?;
            ensure!(action["agent"]==job.agent && action["connection"]==input.connection && action["operation"]==input.operation && action["arguments"]==input.arguments,"Action request ID was already used for different work");
            let visible=visible(&tx,&job.workspace,action["id"].as_str().unwrap()).await?;
            tx.commit().await?;
            return Ok(Proposed{action:visible,dispatch:None});
        }
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS count FROM relay_control_actions WHERE workspace=$1 AND job=$2",
            vec![job.workspace.clone().into(), job.reply.clone().into()],
        )
        .await?
        .unwrap()
        .try_get("", "count")?;
        ensure!(
            count < 20,
            "This task reached its supported operation limit"
        );
        let connection=query(&tx,"SELECT server,definition,fingerprint,revision FROM relay_control_connections WHERE workspace=$1 AND id=$2 AND enabled",vec![job.workspace.clone().into(),input.connection.clone().into()]).await?.context("Operation is not assigned or connected")?;
        let definition: Value = connection.try_get("", "definition")?;
        let operation: Operation = serde_json::from_value(
            definition["operations"]
                .as_array()
                .context("Unsupported connection")?
                .iter()
                .find(|op| op["id"] == input.operation)
                .cloned()
                .context("Unsupported operation")?,
        )?;
        operation.arguments(&input.arguments)?;
        if operation.effect == "write" {
            ensure!(query(&tx,"SELECT id FROM relay_control_actions WHERE workspace=$1 AND connection=$2 AND effect='write' AND state IN ('executing','uncertain') LIMIT 1",vec![job.workspace.clone().into(),input.connection.clone().into()]).await?.is_none(),"An earlier write outcome needs reconciliation before another write");
        }
        let rule=query(&tx,"SELECT mode,approver,revision FROM relay_control_rules WHERE workspace=$1 AND agent=$2 AND connection=$3 AND operation=$4 AND chat=$5",vec![job.workspace.clone().into(),job.agent.clone().into(),input.connection.clone().into(),input.operation.clone().into(),job.chat.clone().into()]).await?;
        let (mode, approver, revision) = if let Some(rule) = rule {
            (
                rule.try_get::<String>("", "mode")?,
                rule.try_get::<String>("", "approver")?,
                rule.try_get::<i64>("", "revision")?,
            )
        } else {
            ("block".into(), job.requester.clone(), 0)
        };
        let state = if mode == "block" { "denied" } else { "pending" };
        let id = uuid::Uuid::new_v4().to_string();
        execute(&tx,"INSERT INTO relay_control_actions(id,workspace,agent,chat,thread,job,requester,request_id,connection,operation,arguments,title,effect,rule_revision,connection_revision,approver,state,created,updated) SELECT $1,$2,$3,$4,m.thread,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,floor(extract(epoch FROM clock_timestamp()))::bigint,floor(extract(epoch FROM clock_timestamp()))::bigint FROM relay_chat_messages m WHERE m.workspace=$2 AND m.id=$5",vec![id.clone().into(),job.workspace.clone().into(),job.agent.clone().into(),job.chat.clone().into(),job.reply.clone().into(),job.requester.clone().into(),request_id.into(),input.connection.clone().into(),input.operation.clone().into(),input.arguments.clone().into(),operation.title.clone().into(),operation.effect.clone().into(),revision.into(),connection.try_get::<i64>("","revision")?.into(),approver.into(),state.into()]).await?;
        let dispatch = if mode == "allow" {
            let mut dispatch = claim(&tx, &job.workspace, &id).await?;
            dispatch.origin = Some(job.clone());
            Some(dispatch)
        } else {
            None
        };
        let action = visible(&tx, &job.workspace, &id).await?;
        audit(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "control_action_requested",
            &id,
        )
        .await?;
        tx.commit().await?;
        Ok(Proposed { action, dispatch })
    }
    pub(crate) async fn control_decide(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        input: &crate::identity::control::model::Decision,
    ) -> Result<Proposed> {
        crate::identity::worker_control::model::uuid(&input.id)?;
        ensure!(
            ["approve", "reject"].contains(&input.decision.as_str()),
            "Invalid action decision"
        );
        let tx = self.workspace_tx(w).await?;
        let row=query(&tx,"SELECT to_jsonb(a) AS data FROM relay_control_actions a WHERE workspace=$1 AND agent=$2 AND id=$3",vec![w.into(),agent.into(),id.into()]).await?.context("Action unavailable")?;
        let action: Value = row.try_get("", "data")?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        super::chat::access(&tx, user, w, action["chat"].as_str().unwrap()).await?;
        ensure!(
            action["approver"] == user.id && role(&tx, w, &user.id).await? == "Owner",
            "Only the designated workspace owner can decide this action"
        );
        if !action["decision"].is_null() {
            ensure!(
                action["decision"] == input.decision,
                "This action already has a different decision"
            );
            let action = visible(&tx, w, id).await?;
            tx.commit().await?;
            return Ok(Proposed {
                action,
                dispatch: None,
            });
        }
        ensure!(
            action["state"] == "pending",
            "This action is no longer awaiting approval"
        );
        let job = job_for_action(&tx, &action).await?;
        authorize(&tx, &job, false).await?;
        let dispatch = if input.decision == "approve" {
            Some(claim(&tx, w, id).await?)
        } else {
            execute(&tx,"UPDATE relay_control_actions SET state='rejected',updated=floor(extract(epoch FROM clock_timestamp()))::bigint WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?;
            None
        };
        execute(&tx,"UPDATE relay_control_actions SET decision=$3,decision_id=$4 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),input.decision.clone().into(),input.id.clone().into()]).await?;
        audit(&tx, &user.id, Some(w), "control_action_decided", id).await?;
        let action = visible(&tx, w, id).await?;
        tx.commit().await?;
        Ok(Proposed { action, dispatch })
    }
    pub(crate) async fn control_finish(
        &self,
        w: &str,
        dispatch: &Dispatch,
        result: Value,
        success: bool,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        ensure!(execute(&tx,"UPDATE relay_control_actions SET state=$4,result=$5,updated=floor(extract(epoch FROM clock_timestamp()))::bigint WHERE workspace=$1 AND id=$2 AND state='executing' AND dispatch_token=$3",vec![w.into(),dispatch.id.clone().into(),dispatch.token.clone().into(),if success {"completed"}else{"uncertain"}.into(),result.into()]).await?==1,"Action outcome ownership changed");
        let action = visible(&tx, w, &dispatch.id).await?;
        tx.commit().await?;
        Ok(action)
    }
    pub(crate) async fn control_ready(&self, w: &str, dispatch: &Dispatch) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        let row=query(&tx,"SELECT to_jsonb(a) AS data FROM relay_control_actions a JOIN relay_control_rules r ON r.workspace=a.workspace AND r.agent=a.agent AND r.connection=a.connection AND r.operation=a.operation AND r.chat=a.chat JOIN relay_control_connections c ON c.workspace=a.workspace AND c.id=a.connection WHERE a.workspace=$1 AND a.id=$2 AND a.state='executing' AND a.dispatch_token=$3 AND r.revision=a.rule_revision AND c.revision=a.connection_revision AND c.enabled AND r.mode<>'block'",vec![w.into(),dispatch.id.clone().into(),dispatch.token.clone().into()]).await?.context("Action authority changed before dispatch")?;
        let action: Value = row.try_get("", "data")?;
        let job = if let Some(job) = &dispatch.origin {
            job.clone()
        } else {
            job_for_action(&tx, &action).await?
        };
        authorize(&tx, &job, dispatch.origin.is_some()).await?;
        if action["decision"] == "approve" {
            let approver = User {
                id: action["approver"].as_str().unwrap().into(),
                email: String::new(),
                display_name: None,
            };
            manager(&tx, w, &approver.id).await?;
            super::chat::access(&tx, &approver, w, &job.chat).await?;
        }
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn control_preflight_failed(
        &self,
        w: &str,
        dispatch: &Dispatch,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        ensure!(execute(&tx,"UPDATE relay_control_actions SET state='invalidated',result='\"Connection or current authority could not be verified; no operation call was dispatched.\"'::jsonb,updated=floor(extract(epoch FROM clock_timestamp()))::bigint WHERE workspace=$1 AND id=$2 AND state='executing' AND dispatch_token=$3",vec![w.into(),dispatch.id.clone().into(),dispatch.token.clone().into()]).await?==1,"Action outcome ownership changed");
        let action = visible(&tx, w, &dispatch.id).await?;
        tx.commit().await?;
        Ok(action)
    }
}
pub(super) async fn authorize(db: &impl ConnectionTrait, job: &Job, running: bool) -> Result<()> {
    let user = User {
        id: job.requester.clone(),
        email: String::new(),
        display_name: None,
    };
    super::chat::access(db, &user, &job.workspace, &job.chat).await?;
    ensure!(
        super::chat::agent_access(db, &job.requester, &job.workspace, &job.agent).await?,
        "Agent permission changed"
    );
    ensure!(
        super::library::source_allowed(db, job).await?,
        "Library source access changed"
    );
    ensure!(
        query(
            db,
            "SELECT agent FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3",
            vec![
                job.workspace.clone().into(),
                job.chat.clone().into(),
                job.agent.clone().into()
            ]
        )
        .await?
        .is_some(),
        "Agent left this conversation"
    );
    let row=query(db,"SELECT j.state,j.claim,(j.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint) AS lease_active,m.state AS message_state,j.payload FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply WHERE j.workspace=$1 AND j.reply=$2",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.context("Task unavailable")?;
    let payload: Value = row.try_get("", "payload")?;
    ensure!(
        payload["requester"] == job.requester
            && payload["agent"] == job.agent
            && payload["chat"] == job.chat,
        "Task scope mismatch"
    );
    let message: String = row.try_get("", "message_state")?;
    ensure!(
        ["queued", "streaming", "complete"].contains(&message.as_str()),
        "Task was stopped or failed"
    );
    if running {
        ensure!(
            row.try_get::<String>("", "state")? == "running"
                && row.try_get::<Option<String>>("", "claim")? == job.claim
                && row.try_get::<bool>("", "lease_active")?,
            "Task execution ownership expired"
        );
    }
    Ok(())
}
async fn job_for_action(db: &impl ConnectionTrait, action: &Value) -> Result<Job> {
    let row = query(
        db,
        "SELECT payload FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2",
        vec![
            action["workspace"].as_str().unwrap().into(),
            action["job"].as_str().unwrap().into(),
        ],
    )
    .await?
    .context("Task unavailable")?;
    Ok(serde_json::from_value(row.try_get("", "payload")?)?)
}
async fn visible(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<Value> {
    Ok(query(
        db,
        &format!(
            "SELECT {} AS data FROM relay_control_actions a WHERE workspace=$1 AND id=$2",
            super::control_rules::VIEW
        ),
        vec![w.into(), id.into()],
    )
    .await?
    .context("Action unavailable")?
    .try_get("", "data")?)
}
async fn claim(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<Dispatch> {
    let row=query(db,"SELECT a.arguments,a.operation,a.rule_revision,a.connection_revision,r.revision,r.mode,c.revision AS current_connection_revision,c.server,c.fingerprint,c.definition FROM relay_control_actions a JOIN relay_control_rules r ON r.workspace=a.workspace AND r.agent=a.agent AND r.connection=a.connection AND r.operation=a.operation AND r.chat=a.chat JOIN relay_control_connections c ON c.workspace=a.workspace AND c.id=a.connection WHERE a.workspace=$1 AND a.id=$2 AND a.state='pending' AND c.enabled",vec![w.into(),id.into()]).await?.context("Action permission changed")?;
    ensure!(
        row.try_get::<i64>("", "rule_revision")? == row.try_get::<i64>("", "revision")?
            && row.try_get::<i64>("", "connection_revision")?
                == row.try_get::<i64>("", "current_connection_revision")?
            && row.try_get::<String>("", "mode")? != "block",
        "Approval is stale; request a new action"
    );
    let operation: String = row.try_get("", "operation")?;
    let definition: Value = row.try_get("", "definition")?;
    let operation: Operation = serde_json::from_value(
        definition["operations"]
            .as_array()
            .context("Invalid connection")?
            .iter()
            .find(|o| o["id"] == operation)
            .cloned()
            .context("Operation removed")?,
    )?;
    let token = uuid::Uuid::new_v4().to_string();
    if operation.effect == "write" {
        ensure!(query(db,"SELECT id FROM relay_control_actions WHERE workspace=$1 AND connection=(SELECT connection FROM relay_control_actions WHERE workspace=$1 AND id=$2) AND id<>$2 AND effect='write' AND state IN ('executing','uncertain') LIMIT 1",vec![w.into(),id.into()]).await?.is_none(),"An earlier write is in progress or needs reconciliation");
    }
    execute(db,"UPDATE relay_control_actions SET state='executing',dispatch_token=$3,updated=floor(extract(epoch FROM clock_timestamp()))::bigint WHERE workspace=$1 AND id=$2 AND state='pending'",vec![w.into(),id.into(),token.clone().into()]).await?;
    Ok(Dispatch {
        id: id.into(),
        token,
        server: row.try_get("", "server")?,
        fingerprint: row.try_get("", "fingerprint")?,
        arguments: row.try_get("", "arguments")?,
        operation,
        origin: None,
    })
}
