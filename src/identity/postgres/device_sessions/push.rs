//! Push registrations for signed-in phones and what is due to them (schema 116).
use super::*;

/// How long a message waits, so reading it on the web prevents a push.
pub(crate) const DELAY_SECONDS: i64 = 10;
/// How far back a message, approval or question can still be pushed.
pub(crate) const WINDOW_SECONDS: i64 = 600;

/// One notification owed to one person.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Due {
    pub workspace: String,
    pub workspace_name: String,
    pub user: String,
    /// `message`, `approval` or `question`.
    pub kind: String,
    /// The message, action or question id.
    pub reference: String,
    pub chat: String,
    /// `replied`, `mentioned`, `direct`, `channel`, `approval` or `question`.
    pub reason: String,
    pub author: String,
}

/// Expo's token shape (`ExponentPushToken[…]`), and APNs/FCM tokens' characters.
pub(crate) fn valid_tokens(expo: &str, native: Option<&str>) -> bool {
    let expo_ok = (10..=200).contains(&expo.len())
        && (expo.starts_with("ExponentPushToken[") || expo.starts_with("ExpoPushToken["))
        && expo.ends_with(']')
        && expo.bytes().all(|b| b.is_ascii_graphic());
    let native_ok = native.is_none_or(|token| {
        (10..=4096).contains(&token.len())
            && token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-' | b'.'))
    });
    expo_ok && native_ok
}

impl PgStore {
    /// Register the phone behind this bearer token. A token already held by
    /// another session (the same phone signed in again) moves here.
    pub(crate) async fn register_push(
        &self,
        session_hash: &str,
        expo: &str,
        native: Option<&str>,
    ) -> Result<()> {
        ensure!(valid_tokens(expo, native), "Invalid notification token");
        let tx = self.db.begin().await?;
        let device: String = query(
            &tx,
            "SELECT id FROM relay_device_sessions WHERE hash=$1 AND expires>$2",
            vec![session_hash.into(), now().into()],
        )
        .await?
        .context("Sign in again to turn on notifications")?
        .try_get("", "id")?;
        execute(
            &tx,
            "DELETE FROM relay_push_devices WHERE expo_token=$1 AND device<>$2",
            vec![expo.into(), device.clone().into()],
        )
        .await?;
        execute(
            &tx,
            "INSERT INTO relay_push_devices(device,expo_token,native_token,updated) VALUES($1,$2,$3,$4)
             ON CONFLICT(device) DO UPDATE SET expo_token=excluded.expo_token,native_token=excluded.native_token,updated=excluded.updated",
            vec![device.into(), expo.into(), native.map(str::to_owned).into(), now().into()],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Turn notifications off for the phone behind this bearer token.
    pub(crate) async fn remove_push(&self, session_hash: &str) -> Result<()> {
        execute(
            &self.db,
            "DELETE FROM relay_push_devices p USING relay_device_sessions s WHERE s.id=p.device AND s.hash=$1",
            vec![session_hash.into()],
        )
        .await?;
        Ok(())
    }

    /// Notifications owed now, not yet sent to anyone.
    pub(crate) async fn push_due(&self) -> Result<Vec<Due>> {
        let rows = rows(
            &self.db,
            include_str!("push_due.sql"),
            vec![now().into(), DELAY_SECONDS.into(), WINDOW_SECONDS.into()],
        )
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(Due {
                    workspace: row.try_get("", "workspace")?,
                    workspace_name: row.try_get("", "workspace_name")?,
                    user: row.try_get("", "user_id")?,
                    kind: row.try_get("", "kind")?,
                    reference: row.try_get("", "ref")?,
                    chat: row.try_get("", "chat")?,
                    reason: row.try_get("", "reason")?,
                    author: row.try_get("", "author_name")?,
                })
            })
            .collect()
    }

    /// Claim one notification. Only the first claim, by any controller, sends it.
    pub(crate) async fn claim_push(&self, due: &Due) -> Result<bool> {
        let claimed = execute(
            &self.db,
            "INSERT INTO relay_push_sent(workspace,user_id,kind,ref,at) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
            vec![
                due.workspace.clone().into(),
                due.user.clone().into(),
                due.kind.clone().into(),
                due.reference.clone().into(),
                now().into(),
            ],
        )
        .await?;
        Ok(claimed == 1)
    }

    /// The Expo tokens of this person's signed-in phones.
    pub(crate) async fn push_tokens(&self, user: &str) -> Result<Vec<String>> {
        rows(
            &self.db,
            "SELECT p.expo_token FROM relay_push_devices p JOIN relay_device_sessions s ON s.id=p.device
             WHERE s.user_id=$1 AND s.expires>$2 ORDER BY p.updated DESC LIMIT 20",
            vec![user.into(), now().into()],
        )
        .await?
        .into_iter()
        .map(|row| Ok(row.try_get("", "expo_token")?))
        .collect()
    }

    /// Apple or Google said this phone no longer takes notifications.
    pub(crate) async fn forget_push_token(&self, expo: &str) -> Result<()> {
        execute(
            &self.db,
            "DELETE FROM relay_push_devices WHERE expo_token=$1",
            vec![expo.into()],
        )
        .await?;
        Ok(())
    }

    /// The sent log only needs to outlast the window in which items are pushed.
    pub(crate) async fn prune_push_sent(&self) -> Result<()> {
        execute(
            &self.db,
            "DELETE FROM relay_push_sent WHERE at<$1",
            vec![(now() - 2 * 86_400).into()],
        )
        .await?;
        Ok(())
    }
}
