use super::*;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;

/// Whether this person already owns the workspace with this ID.
async fn owned(tx: &DatabaseTransaction, id: &str, user: &str) -> Result<bool> {
    Ok(query(
        tx,
        "SELECT 1 FROM relay_members WHERE workspace=$1 AND user_id=$2 AND role='Owner'",
        vec![id.into(), user.into()],
    )
    .await?
    .is_some())
}
impl PgStore {
    pub async fn login(&self, provider: &str, email: &str, hash: &str, sid: &str) -> Result<User> {
        let tx = self.db.begin().await?;
        let proposed = uuid::Uuid::new_v4().to_string();
        let row = query(&tx, "INSERT INTO relay_users(id,provider,email) VALUES($1,$2,$3) ON CONFLICT(provider) DO UPDATE SET email=excluded.email RETURNING id,display_name",
            vec![proposed.into(), provider.into(), email.into()]).await?.context("Missing user")?;
        let id: String = row.try_get("", "id")?;
        execute(
            &tx,
            "DELETE FROM relay_sessions WHERE expires <= $1",
            vec![now().into()],
        )
        .await?;
        execute(
            &tx,
            "INSERT INTO relay_sessions(hash,user_id,provider_session,expires) VALUES($1,$2,$3,$4)",
            vec![
                hash.into(),
                id.clone().into(),
                sid.into(),
                (now() + 3600).into(),
            ],
        )
        .await?;
        audit(&tx, &id, None, "sign_in", "").await?;
        tx.commit().await?;
        Ok(User {
            id,
            email: email.into(),
            display_name: row.try_get("", "display_name")?,
        })
    }
    pub async fn session(&self, hash: &str) -> Result<Option<(User, String, String)>> {
        let Some(row) = query(
            &self.db,
            "SELECT user_id,provider_session FROM relay_sessions WHERE hash=$1 AND expires>$2",
            vec![hash.into(), now().into()],
        )
        .await?
        else {
            return Ok(None);
        };
        let id: String = row.try_get("", "user_id")?;
        let account = entities::user::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .context("Missing user")?;
        Ok(Some((
            User {
                id: account.id,
                email: account.email,
                display_name: account.display_name,
            },
            account.provider,
            row.try_get("", "provider_session")?,
        )))
    }
    pub async fn logout(&self, hash: &str) -> Result<()> {
        execute(
            &self.db,
            "DELETE FROM relay_sessions WHERE hash=$1",
            vec![hash.into()],
        )
        .await?;
        Ok(())
    }
    pub async fn snapshot(&self, user: &User) -> Result<Value> {
        let workspaces = json_rows(&self.db, "SELECT jsonb_build_object('id',w.id,'name',w.name,'role',m.role,'people',(SELECT count(*) FROM relay_members x WHERE x.workspace=w.id),'agents',(SELECT count(*) FROM relay_shared_agents a WHERE a.workspace=w.id)) AS data FROM relay_workspaces w JOIN relay_members m ON m.workspace=w.id WHERE m.user_id=$1 ORDER BY w.sequence", vec![user.id.clone().into()]).await?;
        let invitations = json_rows(&self.db, "SELECT jsonb_build_object('id',i.id,'name',w.name,'role',i.role,'expires',i.expires,'inviter',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=i.inviter)) AS data FROM relay_invitations i JOIN relay_workspaces w ON w.id=i.workspace WHERE i.email=$1 AND i.status='pending' AND i.expires>$2 AND i.provider IS NOT NULL", vec![user.email.clone().into(), now().into()]).await?;
        Ok(json!({"user":user,"workspaces":workspaces,"invitations":invitations}))
    }
    /// Workspace creation for people who are not platform administrators: the
    /// invite code is checked and used up in the same transaction, so one code
    /// makes exactly one workspace even when two people try it at once.
    pub async fn create_with_code(
        &self,
        user: &User,
        name: &str,
        id: &str,
        code: &str,
    ) -> Result<String> {
        ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid request ID");
        let tx = self.db.begin().await?;
        // A retry of a request that already succeeded returns its workspace.
        if owned(&tx, id, &user.id).await? {
            tx.commit().await?;
            return self.create(user, name, id).await;
        }
        let Some(code_id) = super::workspace_codes::consume(&tx, user, code).await? else {
            // The lock may have waited for a concurrent copy of this same request.
            if owned(&tx, id, &user.id).await? {
                tx.commit().await?;
                return self.create(user, name, id).await;
            }
            super::workspace_codes::failed(tx, user).await?;
            anyhow::bail!(super::workspace_codes::INVALID);
        };
        self.create_in(&tx, user, name, id).await?;
        super::workspace_codes::mark_used(&tx, &code_id, user, id).await?;
        tx.commit().await?;
        Ok(id.into())
    }
    /// Trusted creation (platform administrators, fixtures and simulation).
    pub async fn create(&self, user: &User, name: &str, id: &str) -> Result<String> {
        let tx = self.db.begin().await?;
        self.create_in(&tx, user, name, id).await?;
        tx.commit().await?;
        Ok(id.into())
    }
    async fn create_in(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        name: &str,
        id: &str,
    ) -> Result<()> {
        ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid request ID");
        ensure!(
            !name.trim().is_empty()
                && name.trim().len() <= 80
                && !name.chars().any(char::is_control),
            "Use a workspace name between 1 and 80 characters"
        );
        // Atomic conflict handling also covers simultaneous retries before the workspace exists.
        let inserted = execute(
            tx,
            "INSERT INTO relay_workspaces(id,name) VALUES($1,$2) ON CONFLICT(id) DO NOTHING",
            vec![id.into(), name.trim().into()],
        )
        .await?;
        if inserted == 0 {
            let existing = entities::workspace::Entity::find()
                .filter(entities::workspace::Column::Id.eq(id))
                .one(tx)
                .await?
                .context("Workspace unavailable")?;
            ensure!(
                existing.name == name.trim() && role(tx, id, &user.id).await? == "Owner",
                "Request ID already used"
            );
        } else {
            execute(
                tx,
                "INSERT INTO relay_members(workspace,user_id,role) VALUES($1,$2,'Owner')",
                vec![id.into(), user.id.clone().into()],
            )
            .await?;
            audit(tx, &user.id, Some(id), "workspace_created", id).await?;
        }
        Ok(())
    }
    pub async fn people(&self, user: &User, workspace: &str) -> Result<Value> {
        let tx = self.read_tx(workspace).await?;
        let own_role = manager(&tx, workspace, &user.id).await?;
        let members = json_rows(&tx, "SELECT jsonb_build_object('id',u.id,'email',u.email,'display_name',u.display_name,'role',m.role) AS data FROM relay_members m JOIN relay_users u ON u.id=m.user_id WHERE m.workspace=$1 ORDER BY m.sequence",vec![workspace.into()]).await?;
        let invitations = if own_role == "Owner" {
            json_rows(&tx, "SELECT jsonb_build_object('id',id,'email',email,'role',role,'status',status,'expires',expires) AS data FROM relay_invitations WHERE workspace=$1 AND status NOT IN ('cancelled','accepted') ORDER BY sequence DESC",vec![workspace.into()]).await?
        } else {
            vec![]
        };
        tx.commit().await?;
        Ok(json!({"members":members,"invitations":invitations,"role":own_role}))
    }
}

impl PgStore {
    pub async fn update_profile(&self, user: &User, name: &str) -> Result<User> {
        let name = crate::identity::profile::validate(name)?;
        let tx = self.db.begin().await?;
        let count = execute(
            &tx,
            "UPDATE relay_users SET display_name=$1 WHERE id=$2",
            vec![name.into(), user.id.clone().into()],
        )
        .await?;
        ensure!(count == 1, "Account unavailable");
        audit(&tx, &user.id, None, "profile_updated", &user.id).await?;
        tx.commit().await?;
        Ok(User {
            display_name: Some(name.into()),
            ..user.clone()
        })
    }
}
