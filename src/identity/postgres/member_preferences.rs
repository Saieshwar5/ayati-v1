//! Personal notification defaults (D232 slice 2), stored on the server so they
//! follow the person to every device. Theme stays on the device.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Preferences {
    /// Desktop alerts while Cuckoo is open in a tab.
    pub desktop_alerts: bool,
    /// The bell level for channels that have none of their own.
    pub channel_level: String,
    /// Alerts when an agent the person manages asks for approval.
    pub approval_alerts: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            desktop_alerts: true,
            channel_level: "mentions".into(),
            approval_alerts: true,
        }
    }
}

/// The person's defaults; older schemas and new members get the defaults.
pub(crate) async fn defaults(
    db: &impl ConnectionTrait,
    w: &str,
    user: &str,
) -> Result<Preferences> {
    if query(
        db,
        "SELECT to_regclass('relay_member_preferences') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .is_none_or(|r| !r.try_get::<bool>("", "ready").unwrap_or(false))
    {
        return Ok(Preferences::default());
    }
    let Some(row) = query(db, "SELECT desktop_alerts,channel_level,approval_alerts FROM relay_member_preferences WHERE workspace=$1 AND user_id=$2",
        vec![w.into(), user.into()]).await? else {
        return Ok(Preferences::default());
    };
    Ok(Preferences {
        desktop_alerts: row.try_get("", "desktop_alerts")?,
        channel_level: row.try_get("", "channel_level")?,
        approval_alerts: row.try_get("", "approval_alerts")?,
    })
}

impl PgStore {
    pub(crate) async fn preferences(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let saved = defaults(&tx, w, &user.id).await?;
        tx.commit().await?;
        Ok(json!(saved))
    }

    /// Saves all three choices at once; the page sends the whole set each time.
    pub(crate) async fn save_preferences(
        &self,
        user: &User,
        w: &str,
        input: &Preferences,
    ) -> Result<Value> {
        ensure!(
            crate::identity::postgres::chat_notifications::LEVELS
                .contains(&input.channel_level.as_str()),
            "Choose all, mentions or none"
        );
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        execute(&tx, "INSERT INTO relay_member_preferences(workspace,user_id,desktop_alerts,channel_level,approval_alerts) VALUES($1,$2,$3,$4,$5)
            ON CONFLICT(workspace,user_id) DO UPDATE SET desktop_alerts=EXCLUDED.desktop_alerts,channel_level=EXCLUDED.channel_level,approval_alerts=EXCLUDED.approval_alerts",
            vec![w.into(), user.id.clone().into(), input.desktop_alerts.into(), input.channel_level.clone().into(), input.approval_alerts.into()]).await?;
        tx.commit().await?;
        Ok(json!(input))
    }
}
