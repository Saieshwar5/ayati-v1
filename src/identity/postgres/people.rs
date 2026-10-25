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
        ensure!(assigned == "User", "Invalid role");
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
    pub async fn change(
        &self,
        user: &User,
        workspace: &str,
        target: &str,
        assigned: Option<&str>,
    ) -> Result<()> {
        ensure!(
            assigned.is_none_or(|r| ["Owner", "User"].contains(&r)),
            "Invalid role"
        );
        let tx = self.workspace_tx(workspace).await?;
        manager(&tx, workspace, &user.id).await?;
        let target_role = role(&tx, workspace, target).await?;
        if target_role == "Owner" && assigned != Some("Owner") {
            let count: i64 = query(
                &tx,
                "SELECT count(*) AS count FROM relay_members WHERE workspace=$1 AND role='Owner'",
                vec![workspace.into()],
            )
            .await?
            .context("Workspace unavailable")?
            .try_get("", "count")?;
            ensure!(count > 1, "A workspace must keep at least one Owner");
        }
        // Revoke documents issued under the old role as well as on removal.
        if assigned != Some(target_role.as_str()) {
            super::connection_authority::membership_changed(&tx, workspace, target).await?;
        }
        execute(
            &tx,
            "DELETE FROM relay_member_app_sessions WHERE workspace=$1 AND user_id=$2",
            vec![workspace.into(), target.into()],
        )
        .await?;
        if let Some(assigned) = assigned {
            execute(
                &tx,
                "UPDATE relay_members SET role=$3 WHERE workspace=$1 AND user_id=$2",
                vec![workspace.into(), target.into(), assigned.into()],
            )
            .await?;
        } else {
            // Foreign keys revoke chat membership, resource grants and app sessions atomically.
            execute(
                &tx,
                "DELETE FROM relay_members WHERE workspace=$1 AND user_id=$2",
                vec![workspace.into(), target.into()],
            )
            .await?;
        }
        audit(
            &tx,
            &user.id,
            Some(workspace),
            "membership_changed",
            &format!("{target}:{}", assigned.unwrap_or("removed")),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
