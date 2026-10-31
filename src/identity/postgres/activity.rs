//! Activity (D235): every agent request a person may see, in one feed with
//! filters, totals and details. It reads existing records and never starts work.
//!
//! Who sees a request (D231, D235): the person who asked; anyone who can see the
//! conversation; the agent's managers; and admins. Requests that used Library
//! sources stay with their requester, as on the agent's own request list.
use super::*;
use serde::Deserialize;

#[path = "activity_now.rs"]
mod now;
#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;
use serde_json::json;

/// History filters; every field is optional.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Filters {
    pub agent: Option<String>,
    pub person: Option<String>,
    pub chat: Option<String>,
    /// active, waiting, done, failed, review, stopped.
    pub status: Option<String>,
    /// chat, schedule, trigger, handoff.
    pub kind: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub q: Option<String>,
    pub before: Option<i64>,
    /// Minutes east of UTC, for the requests-per-day chart.
    pub offset: Option<i32>,
}

impl Filters {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.status
                .as_deref()
                .is_none_or(
                    |s| ["active", "waiting", "done", "failed", "review", "stopped"].contains(&s)
                ),
            "Invalid status filter"
        );
        ensure!(
            self.kind
                .as_deref()
                .is_none_or(|k| ["chat", "schedule", "trigger", "handoff"].contains(&k)),
            "Invalid type filter"
        );
        ensure!(
            self.q.as_deref().is_none_or(|q| q.chars().count() <= 200),
            "Search is too long"
        );
        ensure!(
            self.offset.is_none_or(|o| (-840..=840).contains(&o)),
            "Invalid time zone offset"
        );
        ensure!(self.before.is_none_or(|b| b > 0), "Invalid cursor");
        Ok(())
    }
    /// `$1` workspace … `$12` before, in the order `FEED_FILTER` expects.
    pub(super) fn values_for(&self, w: &str, user: &str, admin: bool) -> Vec<sea_orm::Value> {
        self.values(w, user, admin)
    }
    fn values(&self, w: &str, user: &str, admin: bool) -> Vec<sea_orm::Value> {
        let pattern = self.q.as_ref().filter(|q| !q.trim().is_empty()).map(|q| {
            format!(
                "%{}%",
                q.trim()
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        });
        vec![
            w.into(),
            user.into(),
            admin.into(),
            self.agent.clone().into(),
            self.person.clone().into(),
            self.chat.clone().into(),
            self.status.clone().into(),
            self.from.unwrap_or(0).into(),
            self.to.unwrap_or(i64::MAX).into(),
            pattern.into(),
            self.kind.clone().into(),
            self.before.unwrap_or(i64::MAX).into(),
        ]
    }
}

/// Visibility and filters over `relay_chat_jobs j`, its reply `m` and
/// conversation `c`. Shared by the feed, its totals and the agent cards.
const FEED_FILTER: &str = include_str!("activity_filter.sql");
const SOURCE: &str = include_str!("activity_source.sql");
const FEED: &str = include_str!("activity_feed.sql");
const TOTALS: &str = include_str!("activity_totals.sql");

/// A template with its `/*FILTER*/` and `/*SOURCE*/` parts filled in.
pub(super) fn query_with(template: &str) -> String {
    template
        .replace("/*FILTER*/", FEED_FILTER)
        .replace("/*SOURCE*/", SOURCE)
}

impl PgStore {
    /// A transaction for Activity's short interactive reads. Their row estimates
    /// can be large enough to trigger JIT compilation, which costs more than the
    /// query itself (about a second with 81,000 requests).
    pub(super) async fn activity_tx(&self, w: &str, write: bool) -> Result<DatabaseTransaction> {
        let tx = if write {
            self.workspace_tx(w).await?
        } else {
            self.read_tx(w).await?
        };
        tx.execute_unprepared("SET LOCAL jit = off").await?;
        Ok(tx)
    }

    /// One page of requests, newest first, with the same projection as an
    /// agent's request list plus the agent and where the request came from.
    pub(crate) async fn activity_history(
        &self,
        user: &User,
        w: &str,
        filters: &Filters,
    ) -> Result<Value> {
        filters.validate()?;
        let tx = self.activity_tx(w, false).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let mut values = filters.values(w, &user.id, admin);
        values.push(Option::<String>::None.into());
        let records = rows(&tx, &query_with(FEED), values).await?;
        let more = records.len() > 25;
        let mut cursor = None;
        let mut items = Vec::new();
        for row in records.into_iter().take(25) {
            cursor = Some(row.try_get::<i64>("", "sequence")?);
            let agent: String = row.try_get("", "agent")?;
            let extra = json!({
                "agent": agent, "agent_name": row.try_get::<String>("", "agent_name")?,
                "agent_avatar": row.try_get::<Option<String>>("", "agent_avatar")?,
                "kind": row.try_get::<String>("", "kind")?,
                "conversation_kind": row.try_get::<String>("", "conversation_kind")?,
                "in_conversation": row.try_get::<bool>("", "in_conversation")?,
                "created_ms": row.try_get::<i64>("", "created_ms")?,
                "finished_ms": row.try_get::<Option<i64>>("", "finished_ms")?,
            });
            if let Some(mut item) =
                super::agent_requests::project(&tx, user, w, &agent, row).await?
            {
                if let (Some(item), Some(extra)) = (item.as_object_mut(), extra.as_object()) {
                    item.extend(extra.clone());
                }
                items.push(item);
            }
        }
        tx.commit().await?;
        Ok(json!({"items": items, "next_before": if more { cursor } else { None }}))
    }

    /// Totals and charts for the chosen period and filters, counted on the server.
    pub(crate) async fn activity_totals(
        &self,
        user: &User,
        w: &str,
        filters: &Filters,
    ) -> Result<Value> {
        filters.validate()?;
        let tx = self.activity_tx(w, false).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let mut values = filters.values(w, &user.id, admin);
        values.push(filters.offset.unwrap_or(0).into());
        let row = query(&tx, &query_with(TOTALS), values)
            .await?
            .context("Totals unavailable")?;
        let totals: Value = row.try_get("", "data")?;
        tx.commit().await?;
        Ok(totals)
    }

    /// Agent setting changes, as readable sentences, for agents the person can see.
    pub(crate) async fn activity_changes(
        &self,
        user: &User,
        w: &str,
        agent: Option<&str>,
        before: Option<i64>,
    ) -> Result<Value> {
        ensure!(before.is_none_or(|b| b > 0), "Invalid cursor");
        let tx = self.activity_tx(w, false).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let records = json_rows(&tx, "SELECT jsonb_build_object('id',l.id,'at',l.at,'agent',a.id,'agent_name',a.name,'agent_avatar',a.settings->>'avatar','summary',l.summary,'actor_name',COALESCE(u.display_name,u.email,'Former member')) AS data
            FROM relay_audit l JOIN relay_shared_agents a ON a.workspace=l.workspace AND a.id=l.target
            LEFT JOIN relay_users u ON u.id=l.actor
            WHERE l.workspace=$1 AND l.action IN ('agent_profile_saved','agent_owner_changed') AND l.summary IS NOT NULL AND l.id<$5
              AND ($4::text IS NULL OR a.id=$4)
              AND ($3 OR a.owner_id=$2 OR relay_agent_can_talk(a.workspace,a.id,$2)
                OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.id AND g.user_id=$2))
            ORDER BY l.id DESC LIMIT 26",
            vec![w.into(), user.id.clone().into(), admin.into(), agent.map(str::to_owned).into(), before.unwrap_or(i64::MAX).into()]).await?;
        let more = records.len() > 25;
        let items: Vec<Value> = records.into_iter().take(25).collect();
        let next = if more {
            items.last().map(|c| c["id"].clone())
        } else {
            None
        };
        tx.commit().await?;
        Ok(json!({"changes": items, "next_before": next}))
    }
}
