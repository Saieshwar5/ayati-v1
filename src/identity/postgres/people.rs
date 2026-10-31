use super::*;
impl PgStore {
    pub async fn reserve_invite(
        &self,
        user: &User,
        workspace: &str,
        email: &str,
        assigned: &str,
    ) -> Result<String> {
        ensure!(
            email.len() <= 254 && email.contains('@') && !email.chars().any(char::is_whitespace),
            "Enter a valid email"
        );
        ensure!(assigned == "Member", "Invalid role");
        let tx = self.workspace_tx(workspace).await?;
        manager(&tx, workspace, &user.id).await?;
        ensure!(query(&tx,"SELECT u.id FROM relay_members m JOIN relay_users u ON u.id=m.user_id WHERE m.workspace=$1 AND u.email=$2",vec![workspace.into(),email.into()]).await?.is_none(),"This person is already a member");
        execute(&tx,"UPDATE relay_invitations SET status='expired' WHERE workspace=$1 AND expires<=$2 AND status='pending'",vec![workspace.into(),now().into()]).await?;
        ensure!(query(&tx,"SELECT id FROM relay_invitations WHERE workspace=$1 AND email=$2 AND status IN ('pending','delivery_unknown')",vec![workspace.into(),email.into()]).await?.is_none(), "An invitation is pending or delivery is uncertain; cancel it before sending another");
        let id = uuid::Uuid::new_v4().to_string();
        execute(&tx,"INSERT INTO relay_invitations(id,workspace,email,role,inviter,expires,status) VALUES($1,$2,$3,$4,$5,$6,'pending')",vec![id.clone().into(),workspace.into(),email.into(),assigned.into(),user.id.clone().into(),(now()+604800).into()]).await.context("An invitation is already pending")?;
        audit(&tx, &user.id, Some(workspace), "invitation_created", &id).await?;
        tx.commit().await?;
        Ok(id)
    }
    pub async fn delivered(&self, id: &str, provider: Option<&str>) -> Result<()> {
        execute(
            &self.db,
            "UPDATE relay_invitations SET provider=$2,status=$3 WHERE id=$1 AND status='pending'",
            vec![
                id.into(),
                provider.into(),
                if provider.is_some() {
                    "pending"
                } else {
                    "delivery_unknown"
                }
                .into(),
            ],
        )
        .await?;
        Ok(())
    }
    pub async fn accept(&self, user: &User, id: &str) -> Result<String> {
        let row = query(
            &self.db,
            "SELECT workspace FROM relay_invitations WHERE id=$1 AND email=$2",
            vec![id.into(), user.email.clone().into()],
        )
        .await?
        .context("Invitation unavailable for this account")?;
        let workspace: String = row.try_get("", "workspace")?;
        let tx = self.workspace_tx(&workspace).await?;
        let row=query(&tx,"SELECT role,inviter FROM relay_invitations WHERE id=$1 AND email=$2 AND status='pending' AND expires>$3 AND provider IS NOT NULL FOR UPDATE",vec![id.into(),user.email.clone().into(),now().into()]).await?.context("Invitation unavailable for this account")?;
        let assigned: String = row.try_get("", "role")?;
        let inviter: String = row.try_get("", "inviter")?;
        manager(&tx, &workspace, &inviter).await?;
        execute(&tx,"INSERT INTO relay_members(workspace,user_id,role) VALUES($1,$2,$3) ON CONFLICT(workspace,user_id) DO NOTHING",vec![workspace.clone().into(),user.id.clone().into(),assigned.into()]).await?;
        execute(
            &tx,
            "UPDATE relay_invitations SET status='accepted' WHERE id=$1",
            vec![id.into()],
        )
        .await?;
        audit(&tx, &user.id, Some(&workspace), "invitation_accepted", id).await?;
        tx.commit().await?;
        Ok(workspace)
    }
    /// The invited person turns an invitation down; returns the WorkOS invitation to revoke.
    pub async fn decline(&self, user: &User, id: &str) -> Result<Option<String>> {
        let tx = self.db.begin().await?;
        let row = query(&tx, "UPDATE relay_invitations SET status='cancelled' WHERE id=$1 AND email=$2 AND status IN ('pending','delivery_unknown') RETURNING workspace,provider",
            vec![id.into(), user.email.clone().into()]).await?.context("Invitation unavailable for this account")?;
        let workspace: String = row.try_get("", "workspace")?;
        audit(&tx, &user.id, Some(&workspace), "invitation_declined", id).await?;
        tx.commit().await?;
        Ok(row.try_get("", "provider")?)
    }
    pub async fn cancel(&self, user: &User, workspace: &str, id: &str) -> Result<Option<String>> {
        let tx = self.workspace_tx(workspace).await?;
        manager(&tx, workspace, &user.id).await?;
        let row = query(
            &tx,
            "SELECT provider FROM relay_invitations WHERE id=$1 AND workspace=$2",
            vec![id.into(), workspace.into()],
        )
        .await?
        .context("Invitation unavailable")?;
        let provider = row.try_get("", "provider")?;
        execute(&tx,"UPDATE relay_invitations SET status='cancelled' WHERE id=$1 AND workspace=$2 AND status!='accepted'",vec![id.into(),workspace.into()]).await?;
        audit(&tx, &user.id, Some(workspace), "invitation_cancelled", id).await?;
        tx.commit().await?;
        Ok(provider)
    }
    /// Changes a role, or removes the person when `assigned` is None. Removal hands
    /// their agents and Library items to `heir` (default: the acting admin) and
    /// returns provider invitation IDs to revoke after commit.
    pub async fn change(
        &self,
        user: &User,
        workspace: &str,
        target: &str,
        assigned: Option<&str>,
        heir: Option<&str>,
    ) -> Result<Vec<String>> {
        ensure!(
            assigned.is_none_or(|r| ["Admin", "Member"].contains(&r)),
            "Invalid role"
        );
        // Anyone may leave on their own (naming who takes over); every other change needs an admin.
        let leaving = target == user.id && assigned.is_none();
        let tx = self.workspace_tx(workspace).await?;
        if !leaving {
            manager(&tx, workspace, &user.id).await?;
        }
        let target_role = role(&tx, workspace, target).await?;
        if target_role == "Admin" && assigned != Some("Admin") {
            let count: i64 = query(
                &tx,
                "SELECT count(*) AS count FROM relay_members WHERE workspace=$1 AND role='Admin'",
                vec![workspace.into()],
            )
            .await?
            .context("Workspace unavailable")?
            .try_get("", "count")?;
            ensure!(count > 1, "A workspace must keep at least one admin");
        }
        let heir = heir.unwrap_or(&user.id);
        let before = if assigned.is_none() {
            ensure!(heir != target, "Choose someone else to take over");
            role(&tx, workspace, heir)
                .await
                .context("The person taking over must be in this workspace")?;
            Some(super::member_handover::responsibilities(&tx, workspace, target).await?)
        } else {
            None
        };
        let actor_name = super::member_handover::name(&tx, &user.id).await?;
        let target_name = super::member_handover::name(&tx, target).await?;
        // Pending approvals need no change: whoever may decide is checked when
        // they decide (approval_authority::may_decide), so a removed manager's
        // requests stay with the agent's other managers.
        execute(
            &tx,
            "DELETE FROM relay_member_app_sessions WHERE workspace=$1 AND user_id=$2",
            vec![workspace.into(), target.into()],
        )
        .await?;
        let mut providers = Vec::new();
        if let Some(assigned) = assigned {
            execute(
                &tx,
                "UPDATE relay_members SET role=$3 WHERE workspace=$1 AND user_id=$2",
                vec![workspace.into(), target.into(), assigned.into()],
            )
            .await?;
        } else {
            providers = super::member_handover::hand_over(&tx, workspace, target, heir).await?;
            // Foreign keys revoke chat membership, resource grants and app sessions atomically.
            execute(
                &tx,
                "DELETE FROM relay_members WHERE workspace=$1 AND user_id=$2",
                vec![workspace.into(), target.into()],
            )
            .await?;
        }
        super::task_revocation::inaccessible(&tx, workspace).await?;
        let summary = match (assigned, &before) {
            (None, Some(before)) => {
                let heir_name = super::member_handover::name(&tx, heir).await?;
                super::member_handover::removal_summary(
                    (!leaving).then_some(actor_name.as_str()),
                    &target_name,
                    &heir_name,
                    before,
                )
            }
            (Some(role), _) if role == target_role => {
                format!("{actor_name} kept {target_name} as {}", role.to_lowercase())
            }
            (Some(role), _) => format!(
                "{actor_name} made {target_name} {}",
                if role == "Admin" {
                    "an admin"
                } else {
                    "a member"
                }
            ),
            (None, None) => unreachable!("removal always records its responsibilities"),
        };
        audit_with_summary(
            &tx,
            &user.id,
            Some(workspace),
            "membership_changed",
            &format!("{target}:{}", assigned.unwrap_or("removed")),
            &summary,
        )
        .await?;
        tx.commit().await?;
        Ok(providers)
    }
}
