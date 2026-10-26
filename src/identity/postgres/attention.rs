//! Derived, source-scoped work projections and monotonic personal acknowledgments.
use super::*;
use crate::identity::{
    attention::{Acknowledge, Cursor, Page},
    chat::store::Job,
};
use serde_json::json;

impl PgStore {
    pub(crate) async fn attention(&self, user: &User, w: &str, input: &Page) -> Result<Value> {
        let mut cursor = input.cursor()?;
        let view = if input.view.is_empty() {
            "needs_you"
        } else {
            &input.view
        };
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let mut items = Vec::new();
        // The SQL boundary filters current audiences before pagination. Keep the
        // authoritative Rust source check too, including after future changes.
        loop {
            let records = candidates(&tx, user, w, view, &cursor, None).await?;
            let exhausted = records.len() < 26;
            for row in records {
                cursor = cursor_for(&row)?;
                if let Some(item) = project(&tx, user, w, row).await? {
                    items.push(item);
                    if items.len() == 26 {
                        break;
                    }
                }
            }
            if exhausted || items.len() == 26 {
                break;
            }
        }
        let more = items.len() > 25;
        items.truncate(25);
        let next = if more {
            items
                .last()
                .map(|i| serde_json::to_string(&i["cursor"]))
                .transpose()?
        } else {
            None
        };
        for item in &mut items {
            item.as_object_mut().unwrap().remove("cursor");
        }
        tx.commit().await?;
        Ok(json!({"available":true,"items":items,"next_before":next}))
    }
    pub(crate) async fn acknowledge_attention(
        &self,
        user: &User,
        w: &str,
        input: &Acknowledge,
    ) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let cursor = Cursor {
            at: i64::MAX,
            kind: "z".into(),
            id: "z".into(),
        };
        let row = candidates(&tx, user, w, "all", &cursor, Some(input))
            .await?
            .into_iter()
            .next()
            .context("Attention source unavailable")?;
        let item = project(&tx, user, w, row)
            .await?
            .context("Attention source unavailable")?;
        ensure!(
            item["version"] == input.version,
            "Attention changed; refresh before marking it read"
        );
        ensure!(
            !input.reviewed || (input.kind == "outcome" && item["status"] != "complete"),
            "Only a failed or interrupted outcome can be marked reviewed"
        );
        execute(
            &tx,
            include_str!("attention_acknowledge.sql"),
            vec![
                w.into(),
                user.id.clone().into(),
                input.kind.clone().into(),
                input.id.clone().into(),
                input.version.clone().into(),
                input.reviewed.into(),
            ],
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"saved":true}))
    }
}
async fn candidates(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    view: &str,
    cursor: &Cursor,
    exact: Option<&Acknowledge>,
) -> Result<Vec<QueryResult>> {
    rows(
        tx,
        include_str!("attention.sql"),
        vec![
            w.into(),
            user.id.clone().into(),
            view.into(),
            cursor.at.into(),
            cursor.kind.clone().into(),
            cursor.id.clone().into(),
            exact.map(|a| a.kind.clone()).into(),
            exact.map(|a| a.id.clone()).into(),
        ],
    )
    .await
}
fn cursor_for(row: &QueryResult) -> Result<Cursor> {
    Ok(Cursor {
        at: row.try_get("", "at")?,
        kind: row.try_get("", "kind")?,
        id: row.try_get("", "id")?,
    })
}
async fn project(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    row: QueryResult,
) -> Result<Option<Value>> {
    let job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
    ensure!(
        job.workspace == w
            && job.reply == row.try_get::<String>("", "reply")?
            && job.chat == row.try_get::<String>("", "chat")?,
        "Attention source scope mismatch"
    );
    super::approval_authority::agent_visible(tx, &user.id, w, &job.agent).await?;
    if super::questions::readable(tx, user, &job).await.is_err() {
        return Ok(None);
    }
    let kind: String = row.try_get("", "kind")?;
    let id: String = row.try_get("", "id")?;
    if kind == "approval" {
        let action = super::approval_delivery::action(tx, w, &id).await?;
        if action["approver"] != user.id
            || super::approval_delivery::viewer(tx, user, w, &action)
                .await
                .is_err()
        {
            return Ok(None);
        }
    } else if job.requester != user.id {
        return Ok(None);
    }
    Ok(Some(
        json!({"kind":kind,"id":id,"version":row.try_get::<String>("","version")?,
        "title":row.try_get::<String>("","title")?,"status":row.try_get::<String>("","status")?,
        "at":row.try_get::<i64>("","at")?,"agent":job.agent,"agent_name":row.try_get::<String>("","agent_name")?,
        "chat":job.chat,"thread":row.try_get::<Option<String>>("","thread")?,"reply":job.reply,
        "conversation_name":row.try_get::<String>("","conversation_name")?,"archived":row.try_get::<bool>("","archived")?,
        "read":row.try_get::<Option<i64>>("","read_ms")?.is_some(),
        "reviewed":row.try_get::<Option<i64>>("","reviewed_ms")?.is_some(),"cursor":cursor_for(&row)?}),
    ))
}
