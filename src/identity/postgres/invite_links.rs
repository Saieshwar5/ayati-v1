//! Cuckoo invite links (/invite/<secret>): an owner sends the link anywhere; the
//! invited person opens it, signs in with the invited email and accepts. Only the
//! SHA-256 of the secret is stored; it lives exactly as long as the invitation.
use super::*;
use crate::identity::provider::{digest, random_token};
use serde_json::json;

/// `a***@gmail.com`: enough to recognise, not enough to harvest addresses.
fn masked(email: &str) -> String {
    match email.split_once('@') {
        Some((name, domain)) => format!("{}***@{domain}", name.chars().take(1).collect::<String>()),
        None => "***".into(),
    }
}

/// The invitation a link points to: id, workspace, email, status, expiry.
async fn by_link(db: &impl ConnectionTrait, token: &str) -> Result<Option<sea_orm::QueryResult>> {
    query(db, "SELECT i.id,i.workspace,i.email,i.role,i.status,i.expires,w.name,(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=i.inviter) AS inviter
        FROM relay_invitations i JOIN relay_workspaces w ON w.id=i.workspace WHERE i.link_hash=$1",
        vec![digest(token).into()]).await
}

impl PgStore {
    /// Issues (or replaces) the link of a pending invitation. Managers only.
    pub async fn issue_invite_link(
        &self,
        user: &User,
        workspace: &str,
        invitation: &str,
    ) -> Result<String> {
        let tx = self.workspace_tx(workspace).await?;
        manager(&tx, workspace, &user.id).await?;
        let token = random_token();
        let changed = execute(&tx, "UPDATE relay_invitations SET link_hash=$3 WHERE id=$1 AND workspace=$2 AND status IN ('pending','delivery_unknown') AND expires>$4",
            vec![invitation.into(), workspace.into(), digest(&token).into(), now().into()]).await?;
        ensure!(changed == 1, "Only a pending invitation has a link");
        audit(
            &tx,
            &user.id,
            Some(workspace),
            "invitation_link_issued",
            invitation,
        )
        .await?;
        tx.commit().await?;
        Ok(token)
    }

    /// What anyone holding the link may see before signing in.
    pub async fn invite_preview(&self, token: &str, viewer: Option<&User>) -> Result<Value> {
        let Some(row) = by_link(&self.db, token).await? else {
            return Ok(json!({"status": "invalid"}));
        };
        let status: String = row.try_get("", "status")?;
        let expires: i64 = row.try_get("", "expires")?;
        let status = match status.as_str() {
            "pending" | "delivery_unknown" if expires > now() => "pending",
            "pending" | "delivery_unknown" | "expired" => "expired",
            "accepted" => "accepted",
            _ => "withdrawn",
        };
        let email: String = row.try_get("", "email")?;
        let mut preview = json!({
            "status": status, "workspace": row.try_get::<String>("", "name")?,
            "inviter": row.try_get::<Option<String>>("", "inviter")?, "role": row.try_get::<String>("", "role")?,
            "expires": expires, "email_hint": masked(&email),
        });
        if let Some(viewer) = viewer {
            preview["matches"] = json!(viewer.email.eq_ignore_ascii_case(&email));
            if status == "accepted" && viewer.email.eq_ignore_ascii_case(&email) {
                preview["workspace_id"] = json!(row.try_get::<String>("", "workspace")?);
            }
        }
        Ok(preview)
    }

    /// The signed-in person must be the invited email; then it is a normal accept.
    pub async fn accept_invite_link(&self, user: &User, token: &str) -> Result<String> {
        let id = self.link_invitation(user, token).await?;
        self.accept(user, &id).await
    }
    pub async fn decline_invite_link(&self, user: &User, token: &str) -> Result<Option<String>> {
        let id = self.link_invitation(user, token).await?;
        self.decline(user, &id).await
    }
    async fn link_invitation(&self, user: &User, token: &str) -> Result<String> {
        let row = by_link(&self.db, token)
            .await?
            .context("This invitation link is not valid")?;
        let email: String = row.try_get("", "email")?;
        ensure!(
            user.email.eq_ignore_ascii_case(&email),
            "This invitation is for {}",
            masked(&email)
        );
        Ok(row.try_get("", "id")?)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn masked_emails_keep_only_the_first_letter_and_domain() {
        assert_eq!(super::masked("ananya@gmail.com"), "a***@gmail.com");
        assert_eq!(super::masked("broken"), "***");
    }
}
