//! Device sessions (schema 117): one-time sign-in codes made by a web session,
//! redeemed once by a phone for a long-lived device token. Codes and tokens are
//! kept only as hashes. See `runtime/device-sessions.md`.
use super::*;
use crate::identity::provider::{digest, random_token};
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;

/// A sign-in code works for 10 minutes.
pub(crate) const CODE_SECONDS: i64 = 600;
/// A device session lasts 90 days from sign-in, then the phone scans again.
pub(crate) const DEVICE_SECONDS: i64 = 90 * 86_400;
/// Codes one person can make per hour; more is a script, not a person.
const CODES_PER_HOUR: i32 = 10;
/// Signed-in devices per person at once.
const DEVICES_PER_PERSON: i64 = 20;
/// Unknown, used and expired codes all get this answer.
pub(crate) const INVALID_CODE: &str =
    "This sign-in code doesn't work. It may be used or expired: make a new one on the web.";

/// A valid (unrevoked, unexpired) device session and its person.
pub(crate) struct DeviceSession {
    pub id: String,
    pub user: User,
    /// The WorkOS user ID.
    pub provider: String,
    pub last_used: i64,
}

fn device_name(name: &str) -> Result<&str> {
    let name = name.trim();
    ensure!(
        !name.is_empty() && name.chars().count() <= 60 && !name.chars().any(char::is_control),
        "Name the device in 1 to 60 characters"
    );
    Ok(name)
}

impl PgStore {
    /// A new sign-in code for this person, replacing any earlier one. `None`
    /// when the person already made [`CODES_PER_HOUR`] codes this hour.
    pub async fn create_link_code(&self, user: &User) -> Result<Option<(String, i64)>> {
        let code = random_token();
        let (at, expires) = (now(), now() + CODE_SECONDS);
        let tx = self.db.begin().await?;
        let made = query(
            &tx,
            "INSERT INTO relay_device_link_codes(user_id,code_hash,created,expires,used,window_start,window_count)
             VALUES($1,$2,$3,$4,NULL,$3,1)
             ON CONFLICT(user_id) DO UPDATE SET code_hash=excluded.code_hash, created=excluded.created,
              expires=excluded.expires, used=NULL,
              window_start=CASE WHEN relay_device_link_codes.window_start>$5 THEN relay_device_link_codes.window_start ELSE $3 END,
              window_count=CASE WHEN relay_device_link_codes.window_start>$5 THEN relay_device_link_codes.window_count+1 ELSE 1 END
             WHERE relay_device_link_codes.window_start<=$5 OR relay_device_link_codes.window_count<$6
             RETURNING user_id",
            vec![
                user.id.clone().into(),
                digest(&code).into(),
                at.into(),
                expires.into(),
                (at - 3600).into(),
                CODES_PER_HOUR.into(),
            ],
        )
        .await?;
        if made.is_none() {
            return Ok(None);
        }
        audit(&tx, &user.id, None, "device_code_created", "").await?;
        tx.commit().await?;
        Ok(Some((code, expires)))
    }

    /// Uses the code exactly once and signs the device in. Of two concurrent
    /// redeems of one code, the second waits for the first's row lock and then
    /// finds the code used. Returns the device token (shown once) and the device.
    pub async fn redeem_link_code(
        &self,
        code: &str,
        name: &str,
        platform: &str,
    ) -> Result<(String, Value)> {
        let name = device_name(name)?;
        ensure!(
            matches!(platform, "ios" | "android"),
            "Platform must be ios or android"
        );
        ensure!(!code.is_empty() && code.len() <= 128, INVALID_CODE);
        let at = now();
        let tx = self.db.begin().await?;
        let user: String = query(
            &tx,
            "UPDATE relay_device_link_codes SET used=$2 WHERE code_hash=$1 AND used IS NULL AND expires>$2 RETURNING user_id",
            vec![digest(code).into(), at.into()],
        )
        .await?
        .context(INVALID_CODE)?
        .try_get("", "user_id")?;
        // Signed-out and expired devices drop out of the list; keep their rows a month.
        execute(
            &tx,
            "DELETE FROM relay_device_sessions WHERE user_id=$1 AND (revoked<$2 OR expires<$2)",
            vec![user.clone().into(), (at - 30 * 86_400).into()],
        )
        .await?;
        let active: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_device_sessions WHERE user_id=$1 AND revoked IS NULL AND expires>$2",
            vec![user.clone().into(), at.into()],
        )
        .await?
        .context("Missing device count")?
        .try_get("", "n")?;
        // Returning here rolls back, so the code still works after a sign-out.
        ensure!(
            active < DEVICES_PER_PERSON,
            "Too many devices are signed in. Sign one out on the web, then scan again."
        );
        let (id, token) = (uuid::Uuid::new_v4().to_string(), random_token());
        let expires = at + DEVICE_SECONDS;
        execute(
            &tx,
            "INSERT INTO relay_device_sessions(id,user_id,token_hash,name,platform,created,last_used,expires) VALUES($1,$2,$3,$4,$5,$6,$6,$7)",
            vec![
                id.clone().into(),
                user.clone().into(),
                digest(&token).into(),
                name.into(),
                platform.into(),
                at.into(),
                expires.into(),
            ],
        )
        .await?;
        audit(&tx, &user, None, "device_signed_in", &id).await?;
        tx.commit().await?;
        let device = json!({"id":id,"name":name,"platform":platform,"created":at,"last_used":at,"expires":expires});
        Ok((token, device))
    }

    pub(crate) async fn device_session(&self, token_hash: &str) -> Result<Option<DeviceSession>> {
        let Some(row) = query(
            &self.db,
            "SELECT d.id,d.last_used,u.id AS user_id,u.email,u.display_name,u.provider FROM relay_device_sessions d JOIN relay_users u ON u.id=d.user_id WHERE d.token_hash=$1 AND d.revoked IS NULL AND d.expires>$2",
            vec![token_hash.into(), now().into()],
        )
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(DeviceSession {
            id: row.try_get("", "id")?,
            user: User {
                id: row.try_get("", "user_id")?,
                email: row.try_get("", "email")?,
                display_name: row.try_get("", "display_name")?,
            },
            provider: row.try_get("", "provider")?,
            last_used: row.try_get("", "last_used")?,
        }))
    }

    /// Callers touch at most once a minute, so reads are not each a write.
    pub(crate) async fn touch_device(&self, id: &str) -> Result<()> {
        execute(
            &self.db,
            "UPDATE relay_device_sessions SET last_used=$2 WHERE id=$1 AND last_used<$2",
            vec![id.into(), now().into()],
        )
        .await?;
        Ok(())
    }

    /// This person's signed-in devices, newest first; never tokens. `current`
    /// is the token hash of the device asking, when a device asks.
    pub async fn devices(&self, user: &User, current: Option<&str>) -> Result<Value> {
        let devices = json_rows(
            &self.db,
            "SELECT jsonb_build_object('id',id,'name',name,'platform',platform,'created',created,'last_used',last_used,'expires',expires,'current',token_hash IS NOT DISTINCT FROM $3) AS data
             FROM relay_device_sessions WHERE user_id=$1 AND revoked IS NULL AND expires>$2 ORDER BY created DESC",
            vec![user.id.clone().into(), now().into(), current.into()],
        )
        .await?;
        Ok(json!({ "devices": devices }))
    }

    /// Signs out one of this person's own devices. False when there is no such
    /// device for this person (another person's device included).
    pub async fn revoke_device(&self, user: &User, id: &str) -> Result<bool> {
        let tx = self.db.begin().await?;
        let changed = execute(
            &tx,
            "UPDATE relay_device_sessions SET revoked=$3 WHERE id=$1 AND user_id=$2 AND revoked IS NULL AND expires>$3",
            vec![id.into(), user.id.clone().into(), now().into()],
        )
        .await?;
        if changed == 0 {
            return Ok(false);
        }
        audit(&tx, &user.id, None, "device_signed_out", id).await?;
        tx.commit().await?;
        Ok(true)
    }
}

pub(super) struct DeviceSessions;
impl MigrationName for DeviceSessions {
    fn name(&self) -> &str {
        "m20261114_000117_device_sessions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for DeviceSessions {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("device_sessions.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Device sessions require a forward migration".into(),
        ))
    }
}
