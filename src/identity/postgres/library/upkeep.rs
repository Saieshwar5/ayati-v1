//! Keeping the Library current (LB5/LB7): documents can be given a review interval; past
//! it they show as due for review, to people and to agents. Questions agents searched for
//! and found nothing are collected for admins, without who asked, to be answered.
use super::*;
use crate::identity::chat::model::Job;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;

/// Schema 77: review intervals and unanswered questions.
pub(crate) struct LibraryUpkeep;
impl MigrationName for LibraryUpkeep {
    fn name(&self) -> &str {
        "m20261105_000077_library_upkeep"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryUpkeep {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("upkeep.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Library upkeep requires a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Review {
    /// Days between reviews, or none to stop asking.
    pub every_days: Option<i32>,
    /// Marks it reviewed now.
    #[serde(default)]
    pub reviewed: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GapAction {
    pub action: String,
}

/// SQL true when item `i` is past its review date.
pub(super) const DUE: &str = "EXISTS(SELECT 1 FROM relay_library_reviews rv WHERE rv.workspace=i.workspace AND rv.item=i.id AND rv.reviewed+rv.every_days*86400 < floor(extract(epoch FROM clock_timestamp())))";

/// A search that found nothing: remembered once per agent and question, counted.
pub(super) async fn record_gap(tx: &DatabaseTransaction, job: &Job, question: &str) -> Result<()> {
    execute(tx, "INSERT INTO relay_library_gaps(workspace,id,query,agent,first_at,last_at) VALUES($1,$2,$3,$4,$5,$5) ON CONFLICT(workspace,agent,lower(query)) WHERE state='open' DO UPDATE SET asked=relay_library_gaps.asked+1,last_at=EXCLUDED.last_at", vec![job.workspace.clone().into(), uuid::Uuid::new_v4().to_string().into(), question.chars().take(256).collect::<String>().into(), job.agent.clone().into(), now().into()]).await?;
    Ok(())
}

impl PgStore {
    /// The review interval and when it is next due, for the item's detail.
    pub(crate) async fn library_review(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        item(&tx, user, w, id, false).await?;
        let review = json_rows(&tx, "SELECT jsonb_build_object('every_days',every_days,'reviewed',reviewed,'due',reviewed+every_days*86400,'overdue',reviewed+every_days*86400 < floor(extract(epoch FROM clock_timestamp()))) AS data FROM relay_library_reviews WHERE workspace=$1 AND item=$2", vec![w.into(), id.into()]).await?.into_iter().next();
        tx.commit().await?;
        Ok(json!({"review":review}))
    }

    /// Editors set how often an item is reviewed, or mark it reviewed now.
    pub(crate) async fn library_review_set(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Review,
    ) -> Result<Value> {
        ensure!(
            input.every_days.is_none_or(|d| (7..=730).contains(&d)),
            "Review every 7 to 730 days"
        );
        let tx = self.workspace_tx(w).await?;
        let current = item(&tx, user, w, id, false).await?;
        ensure!(current["editable"] == true, "Only editors set reviews");
        let title = current["title"].as_str().unwrap_or("").to_owned();
        let said = match input.every_days {
            None if !input.reviewed => {
                execute(
                    &tx,
                    "DELETE FROM relay_library_reviews WHERE workspace=$1 AND item=$2",
                    vec![w.into(), id.into()],
                )
                .await?;
                format!("Stopped reviews of \"{title}\"")
            }
            every => {
                let changed = query(&tx, "UPDATE relay_library_reviews SET every_days=COALESCE($3,every_days),reviewed=$4,set_by=$5 WHERE workspace=$1 AND item=$2 RETURNING every_days", vec![w.into(), id.into(), every.into(), now().into(), user.id.clone().into()]).await?;
                let days: i32 = match changed {
                    Some(row) => row.try_get("", "every_days")?,
                    None => {
                        let days = every.context("Choose how often to review it")?;
                        execute(&tx, "INSERT INTO relay_library_reviews(workspace,item,every_days,reviewed,set_by) VALUES($1,$2,$3,$4,$5)", vec![w.into(), id.into(), days.into(), now().into(), user.id.clone().into()]).await?;
                        days
                    }
                };
                if input.reviewed {
                    format!("Reviewed \"{title}\"")
                } else {
                    format!("\"{title}\" is reviewed every {days} days")
                }
            }
        };
        audit_with_summary(&tx, &user.id, Some(w), "library_review", id, &said).await?;
        tx.commit().await?;
        Ok(json!({"ok":true}))
    }

    /// Questions agents found nothing for, most asked first. Admins only; who asked is
    /// not shown.
    pub(crate) async fn library_gaps(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins see unanswered questions"
        );
        let gaps = json_rows(&tx, "SELECT jsonb_build_object('id',g.id,'query',g.query,'asked',g.asked,'last_at',g.last_at,'agent',COALESCE(a.name,'An agent')) AS data FROM relay_library_gaps g LEFT JOIN relay_shared_agents a ON a.workspace=g.workspace AND a.id=g.agent WHERE g.workspace=$1 AND g.state='open' ORDER BY g.asked DESC,g.last_at DESC,g.id LIMIT 100", vec![w.into()]).await?;
        tx.commit().await?;
        Ok(json!({"gaps":gaps}))
    }

    pub(crate) async fn library_gap_close(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &GapAction,
    ) -> Result<Value> {
        ensure!(
            ["answered", "dismissed"].contains(&input.action.as_str()),
            "Mark it answered or dismiss it"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins close unanswered questions"
        );
        let query_text: String = query(&tx, "UPDATE relay_library_gaps SET state=$3 WHERE workspace=$1 AND id=$2 AND state='open' RETURNING query", vec![w.into(), id.into(), input.action.clone().into()]).await?.context("Already closed")?.try_get("", "query")?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "library_gap_closed",
            id,
            &format!(
                "{} the unanswered question \"{query_text}\"",
                if input.action == "answered" {
                    "Answered"
                } else {
                    "Dismissed"
                }
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"state":input.action}))
    }
}
