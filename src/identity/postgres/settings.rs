use super::*;
use serde_json::json;
impl PgStore {
    pub async fn workspace_settings(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let name: String = query(
            &tx,
            "SELECT name FROM relay_workspaces WHERE id=$1",
            vec![w.into()],
        )
        .await?
        .context("Workspace unavailable")?
        .try_get("", "name")?;
        tx.commit().await?;
        Ok(json!({"name":name}))
    }
    pub async fn rename_workspace(
        &self,
        user: &User,
        w: &str,
        name: &str,
        previous: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        ensure!(
            execute(
                &tx,
                "UPDATE relay_workspaces SET name=$2 WHERE id=$1 AND name=$3",
                vec![w.into(), name.into(), previous.into()]
            )
            .await?
                == 1,
            "Workspace changed; reload before saving"
        );
        audit(&tx, &user.id, Some(w), "workspace_renamed", name).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn workspace_activity(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let items = json_rows(&tx,"SELECT jsonb_build_object('id',a.id,'actor',COALESCE(u.display_name,u.email,a.actor),'action',a.action,'target',a.target,'at',a.at) AS data FROM relay_audit a LEFT JOIN relay_users u ON u.id=a.actor WHERE a.workspace=$1 ORDER BY a.id DESC LIMIT 100",vec![w.into()]).await?;
        tx.commit().await?;
        Ok(json!({"activity":items}))
    }
}
