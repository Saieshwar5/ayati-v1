//! Model usage reports (MG2): calls, tokens and charges over a period, by agent, model,
//! person and day. Admins see the whole workspace; an agent's owner sees that agent;
//! everyone else sees the use made for them. Our provider costs never appear here.
use super::*;
use serde_json::{Value, json};

/// Whose use a report covers.
pub(crate) struct UsageScope<'a> {
    /// One agent, or every agent.
    pub agent: Option<&'a str>,
    /// Days back from now, 1 to 90.
    pub days: i64,
}

const GROUPS: [(&str, &str); 4] = [
    ("by_agent", "u.agent"),
    ("by_model", "u.model"),
    ("by_person", "COALESCE(u.person,'')"),
    (
        "by_day",
        "to_char(to_timestamp(u.at_ms/1000.0) AT TIME ZONE COALESCE(to_jsonb(w)->>'timezone','UTC'),'YYYY-MM-DD')",
    ),
];

fn totals(row: &QueryResult) -> Result<Value> {
    Ok(json!({
        "calls": row.try_get::<i64>("", "calls")?,
        "input_tokens": row.try_get::<i64>("", "input_tokens")?,
        "output_tokens": row.try_get::<i64>("", "output_tokens")?,
        "charge_usd": row.try_get::<i64>("", "charge")? as f64 / 1e6,
    }))
}

impl PgStore {
    pub(crate) async fn model_usage_report(
        &self,
        user: &User,
        w: &str,
        scope: &UsageScope<'_>,
    ) -> Result<Value> {
        ensure!((1..=90).contains(&scope.days), "Choose 1 to 90 days");
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        if let Some(agent) = scope.agent {
            // Its owner and admins see an agent's use.
            super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        }
        // Without an agent, members see only the use made for them.
        let person = (!admin && scope.agent.is_none()).then(|| user.id.clone());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        let since = now - scope.days * 86_400_000;
        let filter = "FROM relay_model_usage u JOIN relay_workspaces w ON w.id=u.workspace
            WHERE u.workspace=$1 AND u.at_ms>=$2 AND ($3::text IS NULL OR u.agent=$3) AND ($4::text IS NULL OR u.person=$4)";
        let sums = "count(*)::bigint AS calls, COALESCE(sum(u.input_tokens),0)::bigint AS input_tokens,
            COALESCE(sum(u.output_tokens),0)::bigint AS output_tokens, COALESCE(sum(u.charge_micro),0)::bigint AS charge";
        let values = || -> Vec<SqlValue> {
            vec![
                w.into(),
                since.into(),
                scope.agent.map(str::to_owned).into(),
                person.clone().into(),
            ]
        };
        let total = query(&tx, &format!("SELECT {sums} {filter}"), values())
            .await?
            .context("Usage unavailable")?;
        let mut report = json!({"days": scope.days, "since_ms": since, "scope": if scope.agent.is_some() { "agent" } else if person.is_some() { "mine" } else { "workspace" },
            "totals": totals(&total)?});
        for (name, key) in GROUPS {
            let order = if name == "by_day" {
                "key"
            } else {
                "charge DESC, calls DESC, key"
            };
            let rows = rows(
                &tx,
                &format!(
                    "SELECT {key} AS key, {sums} {filter} GROUP BY 1 ORDER BY {order} LIMIT 100"
                ),
                values(),
            )
            .await?;
            let mut list = vec![];
            for row in rows {
                let mut item = totals(&row)?;
                item["key"] = json!(row.try_get::<String>("", "key")?);
                list.push(item);
            }
            report[name] = json!(list);
        }
        // Names for agents and people, read in the same snapshot.
        let names = rows(&tx, "SELECT 'agent' AS kind, a.id, a.name FROM relay_shared_agents a WHERE a.workspace=$1
            UNION ALL SELECT 'person', u.id, COALESCE(u.display_name,u.email,'Former member') FROM relay_users u
            JOIN relay_members m ON m.user_id=u.id AND m.workspace=$1", vec![w.into()]).await?;
        tx.commit().await?;
        let mut agents = serde_json::Map::new();
        let mut people = serde_json::Map::new();
        for row in names {
            let target = if row.try_get::<String>("", "kind")? == "agent" {
                &mut agents
            } else {
                &mut people
            };
            target.insert(
                row.try_get("", "id")?,
                json!(row.try_get::<String>("", "name")?),
            );
        }
        for item in report["by_agent"].as_array_mut().into_iter().flatten() {
            let key = item["key"].as_str().unwrap_or_default();
            item["name"] = if key.is_empty() {
                json!("Library")
            } else {
                agents.get(key).cloned().unwrap_or(json!("Removed agent"))
            };
        }
        for item in report["by_person"].as_array_mut().into_iter().flatten() {
            let key = item["key"].as_str().unwrap_or_default();
            item["name"] = if key.is_empty() {
                json!("Workspace")
            } else {
                people.get(key).cloned().unwrap_or(json!("Former member"))
            };
        }
        Ok(report)
    }
}
