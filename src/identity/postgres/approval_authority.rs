//! Authority to approve an action is separate from configuring an agent or joining a channel.
use super::*;
use crate::identity::control::approval_model::ApproverGrant;

// Approval visibility must not grant or require the separate right to request agent work.
pub(super) async fn agent_visible(
    db: &impl ConnectionTrait,
    user: &str,
    w: &str,
    agent: &str,
) -> Result<bool> {
    role(db, w, user).await?;
    let row = query(
        db,
        "SELECT scope,owner_id FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
        vec![w.into(), agent.into()],
    )
    .await?
    .context("Agent unavailable")?;
    let private = row.try_get::<String>("", "scope")? == "private";
    ensure!(
        !private || row.try_get::<Option<String>>("", "owner_id")?.as_deref() == Some(user),
        "Private agent approval unavailable"
    );
    Ok(private)
}

pub(super) async fn require(
    db: &impl ConnectionTrait,
    w: &str,
    connection: &str,
    operation: &str,
    person: &str,
    revision: Option<i64>,
) -> Result<()> {
    if role(db, w, person).await? == "Owner" {
        ensure!(
            revision.is_none(),
            "Approver authority changed; review the person again"
        );
    } else {
        let grant=query(db,"SELECT g.revision FROM relay_control_approvers g JOIN relay_members m ON m.workspace=g.workspace AND m.user_id=g.granted_by AND m.role='Owner' WHERE g.workspace=$1 AND g.connection=$2 AND g.operation=$3 AND g.user_id=$4 AND g.enabled",vec![w.into(),connection.into(),operation.into(),person.into()]).await?.context("An Owner must authorize this person to approve this account action")?;
        ensure!(
            revision == Some(grant.try_get::<i64>("", "revision")?),
            "Approver authorization changed; review the person again"
        );
    }
    Ok(())
}
pub(super) async fn eligible(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
    connection: &str,
    operation: &str,
    grantor: Option<&str>,
) -> Result<Vec<Value>> {
    let people=json_rows(db,"SELECT jsonb_build_object('id',u.id,'name',COALESCE(u.display_name,u.email),'revision',CASE WHEN m.role='Owner' THEN NULL ELSE g.revision END) AS data FROM relay_members m JOIN relay_users u ON u.id=m.user_id LEFT JOIN relay_control_approvers g ON g.workspace=m.workspace AND g.user_id=m.user_id AND g.connection=$2 AND g.operation=$3 LEFT JOIN relay_members o ON o.workspace=g.workspace AND o.user_id=g.granted_by AND o.role='Owner' WHERE m.workspace=$1 AND (m.role='Owner' OR (g.enabled AND o.user_id IS NOT NULL)) AND ($4::text IS NULL OR m.user_id=$4 OR (g.enabled AND g.granted_by=$4)) ORDER BY m.sequence",vec![w.into(),connection.into(),operation.into(),grantor.into()]).await?;
    let mut allowed = Vec::new();
    for person in people {
        if agent_visible(db, person["id"].as_str().unwrap(), w, agent)
            .await
            .is_ok()
        {
            allowed.push(person);
        }
    }
    Ok(allowed)
}
pub(super) async fn channels(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
) -> Result<Vec<Value>> {
    json_rows(db,"SELECT jsonb_build_object('id',id,'name',name) AS data FROM relay_chats c WHERE workspace=$1 AND kind<>'direct' AND (visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2)) ORDER BY name,id LIMIT 100",vec![w.into(),user.id.clone().into()]).await
}
pub(super) async fn route(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
    source: &str,
    target: &str,
    approver: &str,
) -> Result<()> {
    super::chat::access(db, user, w, target).await?;
    let person = User {
        id: approver.into(),
        email: String::new(),
        display_name: None,
    };
    super::chat::access(db, &person, w, source)
        .await
        .context("The selected approver cannot view the work conversation")?;
    super::chat::access(db, &person, w, target)
        .await
        .context("The selected approver cannot view the approval channel")?;
    if source == target {
        return Ok(());
    }
    let private = agent_visible(db, &user.id, w, agent).await?;
    ensure!(
        !private,
        "Private agents keep approval requests in their private work conversation"
    );
    let row=query(db,"SELECT s.visibility AS source_visibility,t.visibility AS target_visibility,t.kind FROM relay_chats s JOIN relay_chats t ON t.workspace=s.workspace WHERE s.workspace=$1 AND s.id=$2 AND t.id=$3",vec![w.into(),source.into(),target.into()]).await?.context("Approval channel unavailable")?;
    ensure!(
        row.try_get::<String>("", "kind")? != "direct",
        "Choose an internal approval channel"
    );
    ensure!(row.try_get::<String>("","source_visibility")?=="public" || (row.try_get::<String>("","target_visibility")?=="private" && query(db,"SELECT p.user_id FROM relay_chat_people p WHERE p.workspace=$1 AND p.chat=$3 AND NOT EXISTS(SELECT 1 FROM relay_chat_people s WHERE s.workspace=p.workspace AND s.chat=$2 AND s.user_id=p.user_id) LIMIT 1",vec![w.into(),source.into(),target.into()]).await?.is_none()),"The approval channel has a broader audience than the work conversation");
    Ok(())
}
pub(super) async fn membership_changed(
    db: &impl ConnectionTrait,
    w: &str,
    user: &str,
) -> Result<()> {
    execute(db,"UPDATE relay_control_actions a SET state='invalidated',result='\"Approval authority changed; request a new action.\"'::jsonb WHERE a.workspace=$1 AND a.state='pending' AND (a.approver=$2 OR EXISTS(SELECT 1 FROM relay_control_approvers g WHERE g.workspace=a.workspace AND g.connection=a.connection AND g.operation=a.operation AND g.user_id=a.approver AND g.granted_by=$2))",vec![w.into(),user.into()]).await?;
    execute(db,"UPDATE relay_control_approvers SET enabled=false,revision=revision+1 WHERE workspace=$1 AND (user_id=$2 OR granted_by=$2)",vec![w.into(),user.into()]).await?;
    execute(db,"UPDATE relay_control_drafts SET preview_revision=NULL WHERE workspace=$1 AND state='draft'",vec![w.into()]).await?;
    Ok(())
}
impl PgStore {
    pub(crate) async fn control_approver_grant(
        &self,
        user: &User,
        w: &str,
        input: &ApproverGrant,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        ensure!(
            role(&tx, w, &input.user).await? == "User",
            "Owners can already be designated as approvers"
        );
        let row=query(&tx,"SELECT c.definition,c.revision FROM relay_control_connections c JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server WHERE c.workspace=$1 AND c.id=$2 AND c.enabled AND s.approved",vec![w.into(),input.connection.clone().into()]).await?.context("Approved connected account required")?;
        ensure!(
            row.try_get::<i64>("", "revision")? == input.connection_revision,
            "Account changed; reload before authorizing"
        );
        let definition: Value = row.try_get("", "definition")?;
        ensure!(
            definition["operations"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|op| op["id"] == input.operation)),
            "Unsupported operation"
        );
        let old=query(&tx,"SELECT enabled,revision,granted_by FROM relay_control_approvers WHERE workspace=$1 AND connection=$2 AND operation=$3 AND user_id=$4",vec![w.into(),input.connection.clone().into(),input.operation.clone().into(),input.user.clone().into()]).await?;
        let rev = old
            .as_ref()
            .map(|r| r.try_get::<i64>("", "revision"))
            .transpose()?
            .unwrap_or(0);
        let same = old.as_ref().is_some_and(|r| {
            r.try_get::<bool>("", "enabled").ok() == Some(input.enabled)
                && r.try_get::<String>("", "granted_by").ok().as_deref() == Some(&user.id)
        });
        if super::workspace_connections::stale_revision(rev, input.revision, same)? {
            tx.commit().await?;
            return Ok(());
        }
        execute(&tx,"INSERT INTO relay_control_approvers(workspace,connection,operation,user_id,enabled,granted_by,revision) VALUES($1,$2,$3,$4,$5,$6,1) ON CONFLICT(workspace,connection,operation,user_id) DO UPDATE SET enabled=excluded.enabled,granted_by=excluded.granted_by,revision=relay_control_approvers.revision+1",vec![w.into(),input.connection.clone().into(),input.operation.clone().into(),input.user.clone().into(),input.enabled.into(),user.id.clone().into()]).await?;
        execute(&tx,"UPDATE relay_control_rules SET mode='block',revision=revision+1 WHERE workspace=$1 AND connection=$2 AND operation=$3 AND approver=$4",vec![w.into(),input.connection.clone().into(),input.operation.clone().into(),input.user.clone().into()]).await?;
        execute(&tx,"UPDATE relay_control_actions SET state='invalidated',result='\"Approval authorization changed.\"'::jsonb WHERE workspace=$1 AND connection=$2 AND operation=$3 AND approver=$4 AND state='pending'",vec![w.into(),input.connection.clone().into(),input.operation.clone().into(),input.user.clone().into()]).await?;
        execute(&tx,"UPDATE relay_control_drafts SET preview_revision=NULL WHERE workspace=$1 AND state='draft'",vec![w.into()]).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            "control_approver_authorized",
            &format!("{}/{}/{}", input.connection, input.operation, input.user),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
