//! Device sessions for the phone app (schema 115).
//!
//! A sign-in the app starts ends with a one-time code (60 seconds, single use)
//! that the app redeems with its PKCE verifier for a random bearer token. Only
//! digests are stored. A device session carries the WorkOS session its sign-in
//! created, so every request checks WorkOS exactly as a browser session does.
use super::*;
use crate::identity::digest;

#[cfg(test)]
mod device_tests;
pub(crate) mod migration;
pub(crate) mod push;
#[cfg(test)]
mod push_tests;
#[cfg(test)]
mod test_fixture;
#[cfg(test)]
mod tests;

/// A device stays signed in for 30 days, or until its WorkOS session ends.
pub(crate) const DEVICE_SECONDS: i64 = 30 * 24 * 3600;
pub(crate) const CODE_SECONDS: i64 = 60;
/// `last_used` is written at most this often, not on every request.
const LAST_USED_STEP: i64 = 300;
/// One message for a wrong, expired or reused code, so none can be told apart.
pub(crate) const INVALID_CODE: &str = "Sign-in code is invalid or expired. Sign in again.";

/// The phone the app says it runs on, already validated.
pub(crate) struct Device {
    pub name: String,
    pub platform: String,
}

/// A newly created device session.
pub(crate) struct Issued {
    pub id: String,
    pub expires: i64,
}

impl PgStore {
    /// After WorkOS signs the person in for the app: record who signed in and
    /// the app's PKCE challenge under the code's digest.
    pub(crate) async fn issue_mobile_code(
        &self,
        provider: &str,
        email: &str,
        code_hash: &str,
        provider_session: &str,
        challenge: &str,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        let (user, _) = super::accounts::upsert_user(&tx, provider, email).await?;
        execute(
            &tx,
            "DELETE FROM relay_mobile_codes WHERE expires <= $1",
            vec![now().into()],
        )
        .await?;
        execute(
            &tx,
            "INSERT INTO relay_mobile_codes(hash,user_id,provider_session,challenge,expires) VALUES($1,$2,$3,$4,$5)",
            vec![
                code_hash.into(),
                user.into(),
                provider_session.into(),
                challenge.into(),
                (now() + CODE_SECONDS).into(),
            ],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Redeem a code with the verifier whose digest is its challenge. The code
    /// is deleted whether or not the verifier matches.
    pub(crate) async fn redeem_mobile_code(
        &self,
        code_hash: &str,
        verifier: &str,
        token_hash: &str,
        device: &Device,
    ) -> Result<Issued> {
        let tx = self.db.begin().await?;
        let row = query(
            &tx,
            "DELETE FROM relay_mobile_codes WHERE hash=$1 RETURNING user_id,provider_session,challenge,expires",
            vec![code_hash.into()],
        )
        .await?;
        let valid = match &row {
            Some(row) => {
                let challenge: String = row.try_get("", "challenge")?;
                let expires: i64 = row.try_get("", "expires")?;
                expires > now() && digest(verifier) == challenge
            }
            None => false,
        };
        let Some(row) = row.filter(|_| valid) else {
            // Keep the deletion: a code is single use, right or wrong.
            tx.commit().await?;
            anyhow::bail!(INVALID_CODE);
        };
        let user: String = row.try_get("", "user_id")?;
        let provider_session: String = row.try_get("", "provider_session")?;
        let id = uuid::Uuid::new_v4().to_string();
        let expires = now() + DEVICE_SECONDS;
        execute(
            &tx,
            "DELETE FROM relay_device_sessions WHERE expires <= $1",
            vec![now().into()],
        )
        .await?;
        execute(
            &tx,
            "INSERT INTO relay_device_sessions(id,hash,user_id,provider_session,device_name,platform,created,last_used,expires) VALUES($1,$2,$3,$4,$5,$6,$7,$7,$8)",
            vec![
                id.clone().into(),
                token_hash.into(),
                user.clone().into(),
                provider_session.into(),
                device.name.clone().into(),
                device.platform.clone().into(),
                now().into(),
                expires.into(),
            ],
        )
        .await?;
        audit(&tx, &user, None, "device_signed_in", &id).await?;
        tx.commit().await?;
        Ok(Issued { id, expires })
    }

    /// The account behind a bearer token, with its WorkOS user and session,
    /// in the same shape as [`PgStore::session`].
    pub async fn device_session(&self, hash: &str) -> Result<Option<(User, String, String)>> {
        let Some(row) = query(
            &self.db,
            "SELECT d.id,d.provider_session,d.last_used,u.id AS user_id,u.email,u.display_name,u.provider FROM relay_device_sessions d JOIN relay_users u ON u.id=d.user_id WHERE d.hash=$1 AND d.expires>$2",
            vec![hash.into(), now().into()],
        )
        .await?
        else {
            return Ok(None);
        };
        let last_used: i64 = row.try_get("", "last_used")?;
        if last_used < now() - LAST_USED_STEP {
            let id: String = row.try_get("", "id")?;
            execute(
                &self.db,
                "UPDATE relay_device_sessions SET last_used=$1 WHERE id=$2",
                vec![now().into(), id.into()],
            )
            .await?;
        }
        Ok(Some((
            User {
                id: row.try_get("", "user_id")?,
                email: row.try_get("", "email")?,
                display_name: row.try_get("", "display_name")?,
            },
            row.try_get("", "provider")?,
            row.try_get("", "provider_session")?,
        )))
    }

    /// This person's signed-in phones, newest first. `current` is the digest
    /// of the token making the request, if it is a phone.
    pub(crate) async fn devices(&self, user: &User, current: Option<&str>) -> Result<Vec<Value>> {
        json_rows(
            &self.db,
            "SELECT jsonb_build_object('id',d.id,'name',d.device_name,'platform',d.platform,'created',d.created,'last_used',d.last_used,'expires',d.expires,'current',COALESCE(d.hash=$3,false),'notifications',EXISTS(SELECT 1 FROM relay_push_devices p WHERE p.device=d.id)) AS data FROM relay_device_sessions d WHERE d.user_id=$1 AND d.expires>$2 ORDER BY d.created DESC",
            vec![user.id.clone().into(), now().into(), current.map(str::to_owned).into()],
        )
        .await
    }

    /// Sign one of this person's phones out. Returns its WorkOS session so the
    /// caller can end it too; `None` if there is no such device.
    pub(crate) async fn revoke_device(&self, user: &User, id: &str) -> Result<Option<String>> {
        let tx = self.db.begin().await?;
        let row = query(
            &tx,
            "DELETE FROM relay_device_sessions WHERE id=$1 AND user_id=$2 RETURNING provider_session",
            vec![id.into(), user.id.clone().into()],
        )
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        audit(&tx, &user.id, None, "device_revoked", id).await?;
        tx.commit().await?;
        Ok(Some(row.try_get("", "provider_session")?))
    }

    /// The phone signs itself out. Returns its WorkOS session, if it had one.
    pub(crate) async fn logout_device(&self, hash: &str) -> Result<Option<String>> {
        let tx = self.db.begin().await?;
        let row = query(
            &tx,
            "DELETE FROM relay_device_sessions WHERE hash=$1 RETURNING id,user_id,provider_session",
            vec![hash.into()],
        )
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let user: String = row.try_get("", "user_id")?;
        let id: String = row.try_get("", "id")?;
        audit(&tx, &user, None, "device_signed_out", &id).await?;
        tx.commit().await?;
        Ok(Some(row.try_get("", "provider_session")?))
    }
}
