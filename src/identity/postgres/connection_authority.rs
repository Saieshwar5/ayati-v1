//! Explicit creator rights and per-operation delegation ceilings.
use super::workspace_connections::stale_revision;
use super::*;
use crate::identity::control::connections_model::{Creator, Delegation};

pub(super) async fn can_create(db: &impl ConnectionTrait, w: &str, user: &str) -> Result<bool> {
    let owner = role(db, w, user).await? == "Admin";
    Ok(owner || query(db,"SELECT user_id FROM relay_agent_creators WHERE workspace=$1 AND user_id=$2 AND enabled",vec![w.into(),user.into()]).await?.is_some())
}

pub(super) async fn membership_changed(
    db: &impl ConnectionTrait,
    w: &str,
    user: &str,
) -> Result<()> {
    super::approval_authority::membership_changed(db, w, user).await?;
    // Rejoining or a later promotion must not revive an earlier delegation.
    // A removed/recreated grant can reuse revision 1. Require a fresh human preview.
    execute(db,"UPDATE relay_control_drafts d SET preview_revision=NULL WHERE d.workspace=$1 AND d.state='draft' AND (d.user_id=$2 OR EXISTS(SELECT 1 FROM jsonb_array_elements(d.data->'context'->'accounts') a CROSS JOIN LATERAL jsonb_array_elements(a->'permissions') p WHERE p->>'approver'=$2))",vec![w.into(),user.into()]).await?;
    execute(db,"UPDATE relay_control_actions a SET state='invalidated',result='\"Membership authority changed; request a new action.\"'::jsonb WHERE a.workspace=$1 AND a.state='pending' AND (a.approver=$2 OR EXISTS(SELECT 1 FROM relay_control_rules r WHERE r.workspace=a.workspace AND r.agent=a.agent AND r.connection=a.connection AND r.operation=a.operation AND r.configured_by=$2))",vec![w.into(),user.into()]).await?;
    execute(db,"UPDATE relay_control_rules SET mode='block',revision=revision+1 WHERE workspace=$1 AND (configured_by=$2 OR approver=$2)",vec![w.into(),user.into()]).await?;
    execute(db,"UPDATE relay_control_delegations SET ceiling='block',revision=revision+1 WHERE workspace=$1 AND (user_id=$2 OR approver=$2)",vec![w.into(),user.into()]).await?;
    execute(db,"UPDATE relay_agent_creators SET enabled=false,revision=revision+1 WHERE workspace=$1 AND user_id=$2",vec![w.into(),user.into()]).await?;
    Ok(())
}

impl PgStore {
    pub(crate) async fn can_create_workspace_agent(&self, user: &User, w: &str) -> Result<bool> {
        can_create(&self.db, w, &user.id).await
    }

    pub(crate) async fn control_creator(
        &self,
        user: &User,
        w: &str,
        input: &Creator,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        ensure!(
            role(&tx, w, &input.user).await? == "Member",
            "Admins already have creation authority"
        );
        let old = query(
            &tx,
            "SELECT enabled,revision FROM relay_agent_creators WHERE workspace=$1 AND user_id=$2",
            vec![w.into(), input.user.clone().into()],
        )
        .await?;
        let rev = old
            .as_ref()
            .map(|r| r.try_get::<i64>("", "revision"))
            .transpose()?
            .unwrap_or(0);
        let same = old
            .as_ref()
            .map(|r| r.try_get::<bool>("", "enabled"))
            .transpose()?
            == Some(input.enabled);
        if stale_revision(rev, input.revision, same)? {
            tx.commit().await?;
            return Ok(());
        }
        execute(&tx,"INSERT INTO relay_agent_creators(workspace,user_id,enabled,revision) VALUES($1,$2,$3,1) ON CONFLICT(workspace,user_id) DO UPDATE SET enabled=excluded.enabled,revision=relay_agent_creators.revision+1",vec![w.into(),input.user.clone().into(),input.enabled.into()]).await?;
        audit(&tx, &user.id, Some(w), "agent_creator_saved", &input.user).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn control_delegate(
        &self,
        user: &User,
        w: &str,
        input: &Delegation,
    ) -> Result<()> {
        ensure!(
            ["allow", "ask", "block"].contains(&input.ceiling.as_str()),
            "Invalid delegation limit"
        );
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        ensure!(
            role(&tx, w, &input.user).await? == "Member",
            "Admins manage their own operation rules"
        );
        let row=query(&tx,"SELECT c.definition,c.revision FROM relay_control_connections c JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server WHERE c.workspace=$1 AND c.id=$2 AND c.enabled AND s.approved",vec![w.into(),input.connection.clone().into()]).await?.context("Approved connected account required")?;
        ensure!(
            row.try_get::<i64>("", "revision")? == input.connection_revision,
            "Connection changed; review its operations again"
        );
        let definition: Value = row.try_get("", "definition")?;
        ensure!(
            definition["operations"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|op| op["id"] == input.operation)),
            "Unsupported operation"
        );
        let old=query(&tx,"SELECT ceiling,approver,revision FROM relay_control_delegations WHERE workspace=$1 AND connection=$2 AND user_id=$3 AND operation=$4",vec![w.into(),input.connection.clone().into(),input.user.clone().into(),input.operation.clone().into()]).await?;
        let rev = old
            .as_ref()
            .map(|r| r.try_get::<i64>("", "revision"))
            .transpose()?
            .unwrap_or(0);
        let same = if let Some(row) = &old {
            row.try_get::<String>("", "ceiling")? == input.ceiling
                && row.try_get::<String>("", "approver")? == user.id
        } else {
            false
        };
        if stale_revision(rev, input.revision, same)? {
            tx.commit().await?;
            return Ok(());
        }
        execute(&tx,"INSERT INTO relay_control_delegations(workspace,connection,user_id,operation,ceiling,approver,revision) VALUES($1,$2,$3,$4,$5,$6,1) ON CONFLICT(workspace,connection,user_id,operation) DO UPDATE SET ceiling=excluded.ceiling,approver=excluded.approver,revision=relay_control_delegations.revision+1",vec![w.into(),input.connection.clone().into(),input.user.clone().into(),input.operation.clone().into(),input.ceiling.clone().into(),user.id.clone().into()]).await?;
        // A new limit never retroactively upgrades an agent. Review affected rules.
        execute(&tx,"UPDATE relay_control_rules SET mode='block',revision=revision+1 WHERE workspace=$1 AND connection=$2 AND operation=$3 AND configured_by=$4",vec![w.into(),input.connection.clone().into(),input.operation.clone().into(),input.user.clone().into()]).await?;
        execute(&tx,"UPDATE relay_control_actions a SET state='invalidated',result='\"Delegated authority changed; request a new action.\"'::jsonb FROM relay_control_rules r WHERE r.workspace=a.workspace AND r.agent=a.agent AND r.connection=a.connection AND r.operation=a.operation AND a.workspace=$1 AND a.connection=$2 AND a.operation=$3 AND r.configured_by=$4 AND a.state='pending'",vec![w.into(),input.connection.clone().into(),input.operation.clone().into(),input.user.clone().into()]).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            "control_delegation_saved",
            &format!("{}/{}/{}", input.connection, input.user, input.operation),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

pub(super) async fn rule_approver(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    input: &crate::identity::control::model::Rule,
) -> Result<String> {
    if role(db, w, &user.id).await? == "Admin" {
        ensure!(
            input.delegation_revision.is_none(),
            "Reload permissions after a role change"
        );
        return Ok(user.id.clone());
    }
    let row=query(db,"SELECT d.ceiling,d.approver,d.revision FROM relay_control_delegations d JOIN relay_members m ON m.workspace=d.workspace AND m.user_id=d.approver AND m.role='Admin' WHERE d.workspace=$1 AND d.connection=$2 AND d.operation=$3 AND d.user_id=$4",vec![w.into(),input.connection.clone().into(),input.operation.clone().into(),user.id.clone().into()]).await?.context("An admin must delegate this operation before you can configure it")?;
    ensure!(
        input.delegation_revision == Some(row.try_get::<i64>("", "revision")?),
        "Delegation changed; reload permissions"
    );
    let ceiling: String = row.try_get("", "ceiling")?;
    ensure!(
        input.mode == "block" || ceiling == "allow" || (ceiling == "ask" && input.mode == "ask"),
        "This permission exceeds your delegated authority"
    );
    Ok(row.try_get("", "approver")?)
}
