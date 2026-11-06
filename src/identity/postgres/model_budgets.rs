//! Monthly model budgets (MG3): a limit on what a workspace, or one agent, may be
//! charged for models in a calendar month (the workspace's time zone). Past the warning
//! share, people are told; at the limit, new requests are refused and the gateway stops
//! forwarding calls. Admins set budgets; every member can read them.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;
use serde_json::{Value, json};

/// Schema 89: model budgets.
pub(crate) struct ModelBudgets;
impl MigrationName for ModelBudgets {
    fn name(&self) -> &str {
        "m20261106_000089_model_budgets"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ModelBudgets {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(
                "CREATE TABLE relay_model_budgets (
                  workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
                  agent TEXT NOT NULL DEFAULT '',
                  monthly_micro BIGINT NOT NULL CHECK(monthly_micro BETWEEN 1 AND 1000000000000),
                  warn_percent SMALLINT NOT NULL DEFAULT 80 CHECK(warn_percent BETWEEN 1 AND 99),
                  updated_by TEXT NOT NULL, updated_ms BIGINT NOT NULL,
                  PRIMARY KEY(workspace,agent));",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Model budgets require a forward migration".into(),
        ))
    }
}

/// A budget to set: the workspace's (`agent` empty) or one agent's. No amount removes it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BudgetInput {
    #[serde(default)]
    pub agent: String,
    pub monthly_usd: Option<f64>,
    #[serde(default = "warn_default")]
    pub warn_percent: i16,
}
fn warn_default() -> i16 {
    80
}

/// The budgets that apply here, each with this month's charges so far.
const BUDGETS: &str = "WITH zone AS (SELECT COALESCE(to_jsonb(w)->>'timezone','UTC') AS z FROM relay_workspaces w WHERE w.id=$1),
 month AS (SELECT (extract(epoch FROM (date_trunc('month', now() AT TIME ZONE z) AT TIME ZONE z))*1000)::bigint AS start FROM zone)
 SELECT b.agent, b.monthly_micro, b.warn_percent::bigint AS warn_percent, month.start,
  (SELECT COALESCE(sum(u.charge_micro),0)::bigint FROM relay_model_usage u
   WHERE u.workspace=$1 AND u.at_ms>=month.start AND (b.agent='' OR u.agent=b.agent)) AS used
 FROM relay_model_budgets b, month WHERE b.workspace=$1 AND ($2::text IS NULL OR b.agent IN ('',$2))
 ORDER BY b.agent<>'', b.agent";

fn state(row: &QueryResult) -> Result<Value> {
    let limit: i64 = row.try_get("", "monthly_micro")?;
    let used: i64 = row.try_get("", "used")?;
    let warn: i64 = row.try_get("", "warn_percent")?;
    let percent = used.saturating_mul(100) / limit.max(1);
    let state = if used >= limit {
        "reached"
    } else if percent >= warn {
        "warning"
    } else {
        "ok"
    };
    Ok(
        json!({"agent": row.try_get::<String>("", "agent")?, "monthly_usd": limit as f64 / 1e6, "used_usd": used as f64 / 1e6,
        "percent": percent, "warn_percent": warn, "state": state, "month_start_ms": row.try_get::<i64>("", "start")?}),
    )
}

/// Why new model work for this agent must stop, if a budget is used up.
pub(crate) async fn budget_reached(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
) -> Result<Option<String>> {
    // Upgrade tests send messages at schemas before 89, which have no budgets.
    let present = query(
        db,
        "SELECT to_regclass('relay_model_budgets') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Missing budget check")?
    .try_get::<bool>("", "present")?;
    if !present {
        return Ok(None);
    }
    for row in rows(db, BUDGETS, vec![w.into(), Some(agent.to_owned()).into()]).await? {
        let budget = state(&row)?;
        if budget["state"] == "reached" {
            return Ok(Some(if budget["agent"] == "" {
                "This workspace has used its monthly model budget. An admin can raise it in Settings → Usage.".into()
            } else {
                "This agent has used its monthly model budget. An admin can raise it in Settings → Usage.".into()
            }));
        }
    }
    Ok(None)
}

impl PgStore {
    pub(crate) async fn model_budget_reached(
        &self,
        w: &str,
        agent: &str,
    ) -> Result<Option<String>> {
        budget_reached(&self.db, w, agent).await
    }
    /// Every budget in the workspace with this month's use, for any member.
    pub(crate) async fn model_budgets(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let mut list = vec![];
        for row in rows(&tx, BUDGETS, vec![w.into(), None::<String>.into()]).await? {
            list.push(state(&row)?);
        }
        let names = rows(
            &tx,
            "SELECT id,name FROM relay_shared_agents WHERE workspace=$1",
            vec![w.into()],
        )
        .await?;
        tx.commit().await?;
        for budget in &mut list {
            let agent = budget["agent"].as_str().unwrap_or_default().to_owned();
            budget["name"] = if agent.is_empty() {
                json!("Workspace")
            } else {
                names
                    .iter()
                    .find(|r| r.try_get::<String>("", "id").is_ok_and(|id| id == agent))
                    .and_then(|r| r.try_get::<String>("", "name").ok())
                    .map_or(json!("Removed agent"), |n| json!(n))
            };
        }
        Ok(json!({"budgets": list, "can_change": admin}))
    }

    /// Admins set or remove a budget.
    pub(crate) async fn save_model_budget(
        &self,
        user: &User,
        w: &str,
        input: &BudgetInput,
    ) -> Result<Value> {
        ensure!(
            (1..=99).contains(&input.warn_percent),
            "Warn at 1 to 99 percent of the budget"
        );
        let micro = match input.monthly_usd {
            Some(usd) => {
                ensure!(
                    usd.is_finite() && (0.01..=1_000_000.0).contains(&usd),
                    "A monthly budget is $0.01 to $1,000,000"
                );
                Some((usd * 1e6).round() as i64)
            }
            None => None,
        };
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let mut target = "the workspace".to_owned();
        if !input.agent.is_empty() {
            let row = query(
                &tx,
                "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
                vec![w.into(), input.agent.clone().into()],
            )
            .await?
            .context("Agent unavailable")?;
            target = row.try_get("", "name")?;
        }
        let summary = match micro {
            Some(m) => {
                execute(&tx, "INSERT INTO relay_model_budgets(workspace,agent,monthly_micro,warn_percent,updated_by,updated_ms) VALUES($1,$2,$3,$4,$5,$6)
                    ON CONFLICT(workspace,agent) DO UPDATE SET monthly_micro=$3,warn_percent=$4,updated_by=$5,updated_ms=$6",
                    vec![w.into(), input.agent.clone().into(), m.into(), input.warn_percent.into(), user.id.clone().into(), now().into()]).await?;
                format!(
                    "Set the monthly model budget for {target} to ${:.2}",
                    m as f64 / 1e6
                )
            }
            None => {
                execute(
                    &tx,
                    "DELETE FROM relay_model_budgets WHERE workspace=$1 AND agent=$2",
                    vec![w.into(), input.agent.clone().into()],
                )
                .await?;
                format!("Removed the monthly model budget for {target}")
            }
        };
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "model_budget_saved",
            if input.agent.is_empty() {
                w
            } else {
                &input.agent
            },
            &summary,
        )
        .await?;
        tx.commit().await?;
        self.model_budgets(user, w).await
    }
}
