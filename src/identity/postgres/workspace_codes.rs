//! Single-use, expiring codes that allow one new workspace each (invite-only
//! early access). Only platform administrators issue them; the route layer
//! checks that. Codes are stored as hashes and shown once.
use super::*;
use crate::identity::provider::digest;
use serde_json::json;

/// Failed checks allowed per person in the window before a pause (guessing defence).
const ATTEMPTS: i64 = 10;
const WINDOW_SECONDS: i64 = 15 * 60;
/// No 0/O, 1/I/L: codes are read aloud and typed from phones.
const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

pub(super) const INVALID: &str = "Code invalid or expired";

/// `CKO-XXXX-XXXX-XXXX`: 12 random symbols (about 60 bits).
fn new_code() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 12];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("OS randomness unavailable");
    let symbols: String = bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect();
    format!(
        "CKO-{}-{}-{}",
        &symbols[0..4],
        &symbols[4..8],
        &symbols[8..12]
    )
}

/// Case, spaces and dashes do not matter when a person types a code.
fn code_hash(code: &str) -> String {
    let normal: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase();
    digest(&normal)
}

async fn too_many_attempts(tx: &DatabaseTransaction, user: &User) -> Result<bool> {
    execute(
        tx,
        "DELETE FROM relay_workspace_code_attempts WHERE user_id=$1 AND at<$2",
        vec![user.id.clone().into(), (now() - WINDOW_SECONDS).into()],
    )
    .await?;
    let count: i64 = query(
        tx,
        "SELECT count(*) AS n FROM relay_workspace_code_attempts WHERE user_id=$1",
        vec![user.id.clone().into()],
    )
    .await?
    .context("Missing attempt count")?
    .try_get("", "n")?;
    Ok(count >= ATTEMPTS)
}

/// A usable code row, locked until the transaction ends so one code makes one workspace.
async fn usable(tx: &DatabaseTransaction, code: &str) -> Result<Option<(String, i64)>> {
    let row = query(tx, "SELECT id,expires_at FROM relay_workspace_codes WHERE code_hash=$1 AND revoked_at IS NULL AND used_at IS NULL AND expires_at>$2 FOR UPDATE",
        vec![code_hash(code).into(), now().into()]).await?;
    Ok(match row {
        Some(row) => Some((row.try_get("", "id")?, row.try_get("", "expires_at")?)),
        None => None,
    })
}

impl PgStore {
    /// Platform administrators only (checked by the caller). Returns the code once.
    pub async fn issue_workspace_code(&self, admin: &User, note: &str, days: i64) -> Result<Value> {
        let note = note.trim();
        ensure!(
            !note.is_empty() && note.chars().count() <= 120 && !note.chars().any(char::is_control),
            "Add a note of up to 120 characters, such as the company name"
        );
        ensure!(
            (1..=90).contains(&days),
            "Choose an expiry between 1 and 90 days"
        );
        let (id, code) = (uuid::Uuid::new_v4().to_string(), new_code());
        let expires = now() + days * 86400;
        let tx = self.db.begin().await?;
        execute(&tx, "INSERT INTO relay_workspace_codes(id,code_hash,note,created_by,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6)",
            vec![id.clone().into(), code_hash(&code).into(), note.into(), admin.id.clone().into(), now().into(), expires.into()]).await?;
        audit(&tx, &admin.id, None, "workspace_code_issued", &id).await?;
        tx.commit().await?;
        Ok(json!({"id": id, "code": code, "note": note, "expires_at": expires}))
    }

    pub async fn workspace_codes(&self) -> Result<Value> {
        let tx = self.db.begin().await?;
        let codes = json_rows(&tx, "SELECT jsonb_build_object('id',c.id,'note',c.note,'created_at',c.created_at,'expires_at',c.expires_at,
            'status',CASE WHEN c.used_at IS NOT NULL THEN 'used' WHEN c.revoked_at IS NOT NULL THEN 'revoked' WHEN c.expires_at<=$1 THEN 'expired' ELSE 'unused' END,
            'used_at',c.used_at,'used_by',CASE WHEN u.id IS NULL THEN NULL ELSE jsonb_build_object('name',COALESCE(u.display_name,u.email),'email',u.email) END,
            'workspace',CASE WHEN w.id IS NULL THEN NULL ELSE jsonb_build_object('id',w.id,'name',w.name) END) AS data
            FROM relay_workspace_codes c LEFT JOIN relay_users u ON u.id=c.used_by LEFT JOIN relay_workspaces w ON w.id=c.workspace
            ORDER BY c.created_at DESC LIMIT 200", vec![now().into()]).await?;
        tx.commit().await?;
        Ok(json!({"codes": codes}))
    }

    /// Revoking a used code changes nothing; its workspace stays.
    pub async fn revoke_workspace_code(&self, admin: &User, id: &str) -> Result<()> {
        let tx = self.db.begin().await?;
        let changed = execute(&tx, "UPDATE relay_workspace_codes SET revoked_at=$2 WHERE id=$1 AND used_at IS NULL AND revoked_at IS NULL",
            vec![id.into(), now().into()]).await?;
        ensure!(changed == 1, "Only an unused code can be revoked");
        audit(&tx, &admin.id, None, "workspace_code_revoked", id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Checks a code without using it, so the person can then name the workspace.
    pub async fn check_workspace_code(&self, user: &User, code: &str) -> Result<i64> {
        let tx = self.db.begin().await?;
        ensure!(
            !too_many_attempts(&tx, user).await?,
            "Too many attempts. Try again in 15 minutes."
        );
        match usable(&tx, code).await? {
            Some((_, expires)) => {
                tx.commit().await?;
                Ok(expires)
            }
            None => {
                failed(tx, user).await?;
                anyhow::bail!(INVALID)
            }
        }
    }
}

/// Records a failed attempt and commits it even though the request fails.
pub(super) async fn failed(tx: DatabaseTransaction, user: &User) -> Result<()> {
    execute(
        &tx,
        "INSERT INTO relay_workspace_code_attempts(user_id,at) VALUES($1,$2)",
        vec![user.id.clone().into(), now().into()],
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Inside workspace creation: lock and consume the code, or refuse.
pub(super) async fn consume(
    tx: &DatabaseTransaction,
    user: &User,
    code: &str,
) -> Result<Option<String>> {
    if too_many_attempts(tx, user).await? {
        anyhow::bail!("Too many attempts. Try again in 15 minutes.");
    }
    Ok(usable(tx, code).await?.map(|(id, _)| id))
}

pub(super) async fn mark_used(
    tx: &DatabaseTransaction,
    code: &str,
    user: &User,
    workspace: &str,
) -> Result<()> {
    let used = execute(tx, "UPDATE relay_workspace_codes SET used_by=$2,used_at=$3,workspace=$4 WHERE id=$1 AND used_at IS NULL",
        vec![code.into(), user.id.clone().into(), now().into(), workspace.into()]).await?;
    ensure!(used == 1, INVALID);
    audit(tx, &user.id, Some(workspace), "workspace_code_used", code).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn codes_are_readable_and_typing_variations_match() {
        let code = super::new_code();
        assert_eq!(code.len(), 18);
        assert!(code.starts_with("CKO-"));
        assert!(!code[4..].contains(['0', 'O', '1', 'I', 'L']));
        let typed = code.to_lowercase().replace('-', " ");
        assert_eq!(super::code_hash(&code), super::code_hash(&typed));
        assert_ne!(
            super::code_hash(&code),
            super::code_hash(&super::new_code())
        );
    }
}
