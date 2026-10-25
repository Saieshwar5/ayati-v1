//! Workspace owners explicitly assign reviewed operations. Changes fence pending approvals.
use super::*;
use crate::identity::control::model::Rule;
use serde_json::json;

impl PgStore {
    pub(crate) async fn control_disconnect(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        ensure!(execute(&tx,"UPDATE relay_control_connections SET enabled=false,revision=revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?==1,"Connection unavailable");
        super::workspace_connections::reset_connection(&tx, w, id).await?;
        audit(&tx, &user.id, Some(w), "control_disconnected", id).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn control_snapshot(
        &self,
        user: &User,
        w: &str,
        agent: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let profile = super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let owner = role(&tx, w, &user.id).await? == "Owner";
        let conversations=json_rows(&tx,"SELECT jsonb_build_object('id',c.id,'name',c.name) AS data FROM relay_chats c JOIN relay_chat_agents a ON a.workspace=c.workspace AND a.chat=c.id WHERE c.workspace=$1 AND a.agent=$2 AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3)) ORDER BY c.name,c.id LIMIT 100",vec![w.into(),agent.into(),user.id.clone().into()]).await?;
        let can_manage = owner || profile["owner_id"] == user.id;
        let delegations=json_rows(&tx,"SELECT to_jsonb(d)||jsonb_build_object('approver_name',COALESCE(u.display_name,u.email)) AS data FROM relay_control_delegations d JOIN relay_users u ON u.id=d.approver JOIN relay_members m ON m.workspace=d.workspace AND m.user_id=d.approver AND m.role='Owner' WHERE d.workspace=$1 AND d.user_id=$2",vec![w.into(),user.id.clone().into()]).await?;
        let connections = json_rows(&tx,"SELECT jsonb_build_object('id',c.id,'server',c.server,'definition',c.definition,'enabled',c.enabled,'revision',c.revision) AS data FROM relay_control_connections c WHERE c.workspace=$1 AND ($3 OR ($5 AND EXISTS(SELECT 1 FROM relay_control_delegations d JOIN relay_members m ON m.workspace=d.workspace AND m.user_id=d.approver AND m.role='Owner' WHERE d.workspace=c.workspace AND d.connection=c.id AND d.user_id=$4 AND d.ceiling<>'block')) OR EXISTS(SELECT 1 FROM relay_control_rules r WHERE r.workspace=c.workspace AND r.connection=c.id AND r.agent=$2 AND EXISTS(SELECT 1 FROM relay_chats ch WHERE ch.workspace=r.workspace AND ch.id=r.chat AND (ch.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people cp WHERE cp.workspace=ch.workspace AND cp.chat=ch.id AND cp.user_id=$4))))) ORDER BY c.id LIMIT 50",vec![w.into(),agent.into(),owner.into(),user.id.clone().into(),can_manage.into()]).await?;
        let rules = json_rows(&tx,"SELECT to_jsonb(r)||jsonb_build_object('approver_name',COALESCE(u.display_name,u.email)) AS data FROM relay_control_rules r JOIN relay_users u ON u.id=r.approver JOIN relay_chats c ON c.workspace=r.workspace AND c.id=r.chat WHERE r.workspace=$1 AND r.agent=$2 AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3)) ORDER BY r.connection,r.operation",vec![w.into(),agent.into(),user.id.clone().into()]).await?;
        let actions = json_rows(&tx,&format!("SELECT {VIEW} AS data FROM relay_control_actions a JOIN relay_chats c ON c.workspace=a.workspace AND c.id=a.chat WHERE a.workspace=$1 AND a.agent=$2 AND (a.requester=$3 OR NOT EXISTS(SELECT 1 FROM relay_library_sources l WHERE l.workspace=a.workspace AND l.reply=a.job)) AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=a.workspace AND p.chat=a.chat AND p.user_id=$3)) ORDER BY a.created DESC,a.id DESC LIMIT 50"),vec![w.into(),agent.into(),user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(
            json!({"connections":connections,"conversations":conversations,"rules":rules,"actions":actions,"can_manage":can_manage,"can_manage_connections":owner,"delegations":delegations,"viewer":user.id}),
        )
    }
    pub(crate) async fn control_connect(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        definition: Value,
        fingerprint: &str,
    ) -> Result<()> {
        crate::identity::worker_control::model::uuid(id)?;
        let server = definition["id"]
            .as_str()
            .context("Missing server identity")?;
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        super::workspace_connections::approved(&tx, w, server).await?;
        if let Some(row) = query(&tx,"SELECT id,server,fingerprint,enabled FROM relay_control_connections WHERE workspace=$1 AND (id=$2 OR server=$3)",vec![w.into(),id.into(),server.into()]).await? {
            ensure!(row.try_get::<String>("","id")? == id && row.try_get::<String>("","server")? == server,"Connection already exists; reload before connecting");
            if row.try_get::<String>("","fingerprint")? != fingerprint || !row.try_get::<bool>("","enabled")? {
                execute(&tx,"UPDATE relay_control_connections SET definition=$3,fingerprint=$4,enabled=true,revision=revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),definition.into(),fingerprint.into()]).await?;
                super::workspace_connections::reset_connection(&tx,w,id).await?;
            }
        } else {
            execute(&tx,"INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint) VALUES($1,$2,$3,$4,$5)",vec![w.into(),id.into(),server.into(),definition.into(),fingerprint.into()]).await?;
        }
        audit(&tx, &user.id, Some(w), "control_connected", id).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn control_rule(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &Rule,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        let approver = validate_rule(&tx, user, w, agent, input).await?;
        let old = query(&tx,"SELECT revision,mode,approver,chat,configured_by,delegation_revision,approver_revision,approval_chat FROM relay_control_rules WHERE workspace=$1 AND agent=$2 AND connection=$3 AND operation=$4",vec![w.into(),agent.into(),input.connection.clone().into(),input.operation.clone().into()]).await?;
        if let Some(row) = old {
            let revision: i64 = row.try_get("", "revision")?;
            if revision != input.revision {
                ensure!(
                    revision == input.revision + 1
                        && row.try_get::<String>("", "mode")? == input.mode
                        && row.try_get::<String>("", "approver")? == approver
                        && row.try_get::<String>("", "configured_by")? == user.id
                        && row.try_get::<Option<i64>>("", "delegation_revision")?
                            == input.delegation_revision
                        && row.try_get::<Option<i64>>("", "approver_revision")?
                            == if input.mode == "ask" {
                                input.approver_revision
                            } else {
                                None
                            }
                        && row.try_get::<Option<String>>("", "approval_chat")?
                            == if input.mode == "ask" {
                                input.approval_chat.clone()
                            } else {
                                None
                            }
                        && row.try_get::<String>("", "chat")? == input.chat,
                    "Permission changed; reload before saving"
                );
                tx.commit().await?;
                return Ok(());
            }
        } else {
            ensure!(input.revision == 0, "Permission unavailable");
        }
        write_rule(&tx, user, w, agent, input, &approver).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            "control_rule_saved",
            &format!("{agent}/{}/{}", input.connection, input.operation),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn control_enabled(&self, w: &str, agent: &str, chat: &str) -> Result<bool> {
        Ok(query(&self.db,"SELECT r.agent FROM relay_control_effective_rules r JOIN relay_control_connections c ON c.workspace=r.workspace AND c.id=r.connection WHERE r.workspace=$1 AND r.agent=$2 AND r.chat=$3 AND c.enabled AND r.mode IN ('allow','ask') LIMIT 1",vec![w.into(),agent.into(),chat.into()]).await?.is_some())
    }
}
// Used by manual saves and guided preview/activation. Blocking never grants authority.
pub(super) async fn validate_rule(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
    input: &Rule,
) -> Result<String> {
    ensure!(
        ["allow", "ask", "block"].contains(&input.mode.as_str())
            && input.revision >= 0
            && input.revision < i64::MAX,
        "Invalid permission rule"
    );
    let mut approver = if input.mode == "block" {
        user.id.clone()
    } else {
        super::connection_authority::rule_approver(db, user, w, input).await?
    };
    super::agent_profiles::managed_access(db, &user.id, w, agent, true).await?;
    super::chat::access(db, user, w, &input.chat).await?;
    ensure!(
        query(
            db,
            "SELECT agent FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3",
            vec![w.into(), input.chat.clone().into(), agent.into()]
        )
        .await?
        .is_some(),
        "Agent is not in the selected conversation"
    );
    let connection = query(db,"SELECT c.definition FROM relay_control_connections c JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server WHERE c.workspace=$1 AND c.id=$2 AND c.enabled AND s.approved",vec![w.into(),input.connection.clone().into()]).await?.context("Connection unavailable")?;
    let definition: Value = connection.try_get("", "definition")?;
    ensure!(
        definition["operations"]
            .as_array()
            .is_some_and(|a| a.iter().any(|op| op["id"] == input.operation)),
        "Unsupported operation"
    );
    if input.mode == "ask" {
        let grantor = approver.clone();
        approver = input.approver.clone().unwrap_or(approver);
        if role(db, w, &user.id).await? != "Owner" && approver != grantor {
            ensure!(query(db,"SELECT user_id FROM relay_control_approvers WHERE workspace=$1 AND connection=$2 AND operation=$3 AND user_id=$4 AND granted_by=$5 AND enabled",vec![w.into(),input.connection.clone().into(),input.operation.clone().into(),approver.clone().into(),grantor.into()]).await?.is_some(),"Your delegating Owner must authorize the selected approver");
        }
        super::approval_authority::require(
            db,
            w,
            &input.connection,
            &input.operation,
            &approver,
            input.approver_revision,
        )
        .await?;
        let person = User {
            id: approver.clone(),
            email: String::new(),
            display_name: None,
        };
        super::approval_authority::agent_visible(db, &person.id, w, agent).await?;
        super::chat::access(db, &person, w, &input.chat).await.context("The designated approver cannot view this conversation; choose a shared work conversation")?;
    }
    if input.mode == "ask" {
        super::approval_authority::route(
            db,
            user,
            w,
            agent,
            &input.chat,
            input.approval_chat.as_deref().unwrap_or(&input.chat),
            &approver,
        )
        .await?;
    }
    Ok(approver)
}

pub(super) async fn write_rule(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
    input: &Rule,
    approver: &str,
) -> Result<()> {
    execute(db,"INSERT INTO relay_control_rules(workspace,agent,connection,operation,mode,approver,chat,configured_by,delegation_revision,approver_revision,approval_chat,revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,1) ON CONFLICT(workspace,agent,connection,operation) DO UPDATE SET mode=excluded.mode,approver=excluded.approver,chat=excluded.chat,configured_by=excluded.configured_by,delegation_revision=excluded.delegation_revision,approver_revision=excluded.approver_revision,approval_chat=excluded.approval_chat,revision=relay_control_rules.revision+1",vec![w.into(),agent.into(),input.connection.clone().into(),input.operation.clone().into(),input.mode.clone().into(),approver.into(),input.chat.clone().into(),user.id.clone().into(),input.delegation_revision.into(),(if input.mode=="ask" {input.approver_revision} else {None}).into(),(if input.mode=="ask" {input.approval_chat.as_deref()} else {None}).into()]).await?;
    invalidate(db, w, &input.connection, Some((agent, &input.operation))).await
}

pub(super) const VIEW: &str = "jsonb_build_object('id',a.id,'connection',a.connection,'operation',a.operation,'title',a.title,'effect',a.effect,'decision',a.decision,'requester_name',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=a.requester),'conversation_name',(SELECT c.name FROM relay_chats c WHERE c.workspace=a.workspace AND c.id=a.chat),'delivery',(SELECT jsonb_build_object('status',d.status,'reason',d.reason) FROM relay_control_deliveries d WHERE d.workspace=a.workspace AND d.action=a.id),'arguments',a.arguments,'state',CASE WHEN a.state='executing' AND a.updated < floor(extract(epoch FROM clock_timestamp()))::bigint-60 THEN 'uncertain' ELSE a.state END,'result',a.result,'chat',a.chat,'thread',a.thread,'job',a.job,'approver',a.approver,'created',a.created,'updated',a.updated)";
pub(super) async fn invalidate(
    db: &impl ConnectionTrait,
    w: &str,
    connection: &str,
    rule: Option<(&str, &str)>,
) -> Result<()> {
    execute(db,"UPDATE relay_control_actions SET state='invalidated',result='\"Permissions changed; request a new action.\"'::jsonb,updated=floor(extract(epoch FROM clock_timestamp()))::bigint WHERE workspace=$1 AND connection=$2 AND state='pending' AND ($3::text IS NULL OR (agent=$3 AND operation=$4))",vec![w.into(),connection.into(),rule.map(|r| r.0).into(),rule.map(|r| r.1).into()]).await?;
    Ok(())
}
