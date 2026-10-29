use super::*;
use crate::identity::chat::organization_model::{Pin, PinKind, SavePins};
use serde_json::json;
impl PgStore {
    pub(crate) async fn chat_pins(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let data = read_in(&tx, user, w).await?;
        tx.commit().await?;
        Ok(data)
    }
    pub(crate) async fn organized_chats(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let mut data = super::chat::chats_in(&tx, user, w).await?;
        data["preferences"] = read_in(&tx, user, w).await?;
        tx.commit().await?;
        Ok(data)
    }
    pub(crate) async fn save_chat_pins(
        &self,
        user: &User,
        w: &str,
        input: &SavePins,
    ) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        ensure!(
            visible(&tx, user, w, &input.pins).await? == input.pins,
            "A pinned conversation is no longer available. Refresh your conversations."
        );
        execute(&tx,"INSERT INTO relay_chat_preferences(workspace,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING",vec![w.into(),user.id.clone().into()]).await?;
        let row=query(&tx,"SELECT revision,last_request,last_payload FROM relay_chat_preferences WHERE workspace=$1 AND user_id=$2 FOR UPDATE",vec![w.into(),user.id.clone().into()]).await?.context("Preferences unavailable")?;
        let payload = serde_json::to_value(input)?;
        let revision: i64 = row.try_get("", "revision")?;
        let same = row
            .try_get::<Option<String>>("", "last_request")?
            .as_deref()
            == Some(&input.request);
        let revision = if same {
            ensure!(
                row.try_get::<Option<Value>>("", "last_payload")? == Some(payload),
                "Pin request payload changed"
            );
            revision
        } else {
            ensure!(
                revision == input.revision,
                "Pins changed in another session. Refresh before saving again."
            );
            execute(&tx,"UPDATE relay_chat_preferences SET pins=$3,revision=revision+1,last_request=$4,last_payload=$5 WHERE workspace=$1 AND user_id=$2",vec![w.into(),user.id.clone().into(),json!(input.pins).into(),input.request.clone().into(),payload.into()]).await?;
            revision + 1
        };
        tx.commit().await?;
        Ok(json!({"available":true,"revision":revision,"pins":input.pins}))
    }
}
async fn read_in(tx: &DatabaseTransaction, user: &User, w: &str) -> Result<Value> {
    role(tx, w, &user.id).await?;
    let row = query(
        tx,
        "SELECT revision,pins FROM relay_chat_preferences WHERE workspace=$1 AND user_id=$2",
        vec![w.into(), user.id.clone().into()],
    )
    .await?;
    let (revision, pins) = if let Some(row) = row {
        (
            row.try_get::<i64>("", "revision")?,
            serde_json::from_value::<Vec<Pin>>(row.try_get("", "pins")?)?,
        )
    } else {
        (0, vec![])
    };
    let pins = visible(tx, user, w, &pins).await?;
    Ok(json!({"available":true,"revision":revision,"pins":pins}))
}
async fn visible(tx: &DatabaseTransaction, user: &User, w: &str, pins: &[Pin]) -> Result<Vec<Pin>> {
    role(tx, w, &user.id).await?;
    let ids = pins
        .iter()
        .filter(|p| p.kind == PinKind::Chat)
        .map(|p| p.id.clone())
        .collect::<Vec<_>>();
    let accessible=rows(tx,"SELECT c.id FROM relay_chats c WHERE c.workspace=$1 AND c.id IN(SELECT jsonb_array_elements_text($2::jsonb)) AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3))",vec![w.into(),json!(ids).into(),user.id.clone().into()]).await?.into_iter().map(|r|r.try_get::<String>("","id")).collect::<std::result::Result<std::collections::HashSet<_>,_>>()?;
    Ok(pins
        .iter()
        .filter(|p| accessible.contains(&p.id))
        .cloned()
        .collect())
}
