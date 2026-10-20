use super::*;
impl PgStore {
    pub async fn resource_allowed(
        &self,
        user: &User,
        workspace: &str,
        kind: &str,
        id: &str,
    ) -> Result<Option<String>> {
        let tx = self.read_tx(workspace).await?;
        role(&tx, workspace, &user.id).await?;
        let row=query(&tx,"SELECT access FROM relay_resource_grants WHERE workspace=$1 AND user_id=$2 AND kind=$3 AND resource=$4",vec![workspace.into(),user.id.clone().into(),kind.into(),id.into()]).await?;
        let value = row.map(|r| r.try_get("", "access")).transpose()?;
        tx.commit().await?;
        Ok(value)
    }
    pub async fn resource_ids(
        &self,
        user: &User,
        workspace: &str,
        kind: &str,
    ) -> Result<Vec<String>> {
        let tx = self.read_tx(workspace).await?;
        role(&tx, workspace, &user.id).await?;
        let ids=rows(&tx,"SELECT resource FROM relay_resource_grants WHERE workspace=$1 AND user_id=$2 AND kind=$3",vec![workspace.into(),user.id.clone().into(),kind.into()]).await?.into_iter().map(|r|r.try_get("","resource")).collect::<std::result::Result<Vec<_>,_>>()?;
        tx.commit().await?;
        Ok(ids)
    }
    pub async fn resource_grants(&self, user: &User, workspace: &str) -> Result<Vec<Value>> {
        let tx = self.read_tx(workspace).await?;
        ensure!(
            role(&tx, workspace, &user.id).await? == "Owner",
            "Only the owner can manage resource access"
        );
        let grants=json_rows(&tx,"SELECT jsonb_build_object('user',user_id,'kind',kind,'resource',resource,'access',access) AS data FROM relay_resource_grants WHERE workspace=$1",vec![workspace.into()]).await?;
        tx.commit().await?;
        Ok(grants)
    }
    pub async fn resource_grant(
        &self,
        user: &User,
        workspace: &str,
        target: &str,
        kind: &str,
        resource: &str,
        access: Option<&str>,
    ) -> Result<()> {
        ensure!(["app", "panel"].contains(&kind), "Unknown resource type");
        ensure!(
            access.is_none_or(|a| a == "read" || (kind == "app" && a == "write")),
            "Unknown access type"
        );
        let tx = self.workspace_tx(workspace).await?;
        ensure!(
            role(&tx, workspace, &user.id).await? == "Owner",
            "Only the owner can share resources"
        );
        role(&tx, workspace, target).await?;
        execute(
            &tx,
            "DELETE FROM relay_member_app_sessions WHERE workspace=$1 AND user_id=$2 AND app=$3",
            vec![workspace.into(), target.into(), resource.into()],
        )
        .await?;
        if let Some(access) = access {
            execute(&tx,"INSERT INTO relay_resource_grants(workspace,user_id,kind,resource,access) VALUES($1,$2,$3,$4,$5) ON CONFLICT(workspace,user_id,kind,resource) DO UPDATE SET access=excluded.access",vec![workspace.into(),target.into(),kind.into(),resource.into(),access.into()]).await?;
        } else {
            execute(&tx,"DELETE FROM relay_resource_grants WHERE workspace=$1 AND user_id=$2 AND kind=$3 AND resource=$4",vec![workspace.into(),target.into(),kind.into(),resource.into()]).await?;
        }
        audit(
            &tx,
            &user.id,
            Some(workspace),
            "resource_access_changed",
            &format!("{target}/{resource}"),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn app_session_bound(
        &self,
        user: &User,
        workspace: &str,
        app: &str,
        value: &str,
        document: bool,
    ) -> Result<bool> {
        role(&self.db, workspace, &user.id).await?;
        let sql = if document {
            "SELECT hash FROM relay_member_app_sessions WHERE workspace=$1 AND user_id=$2 AND app=$3 AND document=$4 AND expires>$5"
        } else {
            "SELECT hash FROM relay_member_app_sessions WHERE workspace=$1 AND user_id=$2 AND app=$3 AND hash=$4 AND expires>$5"
        };
        Ok(query(
            &self.db,
            sql,
            vec![
                workspace.into(),
                user.id.clone().into(),
                app.into(),
                value.into(),
                now().into(),
            ],
        )
        .await?
        .is_some())
    }
    pub async fn app_session_save(
        &self,
        user: &User,
        workspace: &str,
        app: &str,
        hash: &str,
        document: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(workspace).await?;
        role(&tx, workspace, &user.id).await?;
        ensure!(query(&tx,"SELECT access FROM relay_resource_grants WHERE workspace=$1 AND user_id=$2 AND kind='app' AND resource=$3",vec![workspace.into(),user.id.clone().into(),app.into()]).await?.is_some(),"App access changed");
        execute(
            &tx,
            "DELETE FROM relay_member_app_sessions WHERE expires<=$1",
            vec![now().into()],
        )
        .await?;
        execute(&tx,"INSERT INTO relay_member_app_sessions(workspace,user_id,app,hash,document,expires) VALUES($1,$2,$3,$4,$5,$6)",vec![workspace.into(),user.id.clone().into(),app.into(),hash.into(),document.into(),(now()+900).into()]).await?;
        tx.commit().await?;
        Ok(())
    }
}
