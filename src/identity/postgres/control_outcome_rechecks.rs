//! Cuckoo asks the provider what became of an unconfirmed write, about 1, 5 and 30
//! minutes after it became unconfirmed (`control::outcome_checks` runs the loop).
//! Several controllers may run it: a check is leased in the workspace's
//! transaction before the provider is asked, and the provider is never asked
//! inside a transaction. Providers switched off are skipped until switched on.
use super::*;
use crate::identity::{connectors::Found, control::model::Operation};
use serde_json::json;

/// Seconds after the action became unconfirmed for each automatic check.
const AFTER: [i64; 3] = [60, 300, 1800];
/// A claimed check is left to another controller if this one hasn't answered by then.
const LEASE_SECONDS: i64 = 120;
const NOW: &str = "floor(extract(epoch FROM clock_timestamp()))::bigint";
/// When the action became unconfirmed: when the provider didn't confirm it, or
/// when its dispatch passed the 60-second rule.
const SINCE: &str = "CASE WHEN a.state='uncertain' THEN a.updated ELSE a.updated+60 END";
/// Kept with a found outcome only while small; the agent reads it as data.
const FOUND_LIMIT: usize = 16384;

/// One leased check: the saved account and the exact action to ask about.
pub(crate) struct Due {
    pub workspace: String,
    pub action: String,
    /// Checks already made; the lease is only good for this attempt.
    pub attempts: i32,
    pub provider: String,
    pub server: String,
    pub binding: Option<Value>,
    pub fingerprint: String,
    /// `None` when the account no longer offers the action.
    pub operation: Option<Operation>,
    pub arguments: Value,
}

/// What the provider could say about a leased check.
pub(crate) enum Asked {
    Found(Found),
    /// The provider can't find outcomes: no more automatic checks.
    CannotCheck,
}

impl PgStore {
    /// Leases up to `limit` checks that are due.
    pub(crate) async fn outcome_checks_due(&self, limit: i64) -> Result<Vec<Due>> {
        let due = rows(&self.db, &format!("SELECT a.workspace,a.id FROM relay_control_actions a
            JOIN relay_control_connections c ON c.workspace=a.workspace AND c.id=a.connection
            LEFT JOIN relay_control_outcome_checks k ON k.workspace=a.workspace AND k.action=a.id
            WHERE a.state IN ('executing','uncertain') AND {UNCONFIRMED} AND a.effect='write' AND a.settled_at IS NULL
            AND COALESCE(k.attempts,0)<3 AND COALESCE(k.lease_until,0)<={NOW} AND COALESCE(k.next_at,{SINCE}+{})<={NOW}
            AND NOT EXISTS(SELECT 1 FROM relay_connector_providers p WHERE p.provider=c.provider AND NOT p.enabled)
            ORDER BY a.updated,a.id LIMIT $1", AFTER[0]), vec![limit.into()]).await?;
        let mut leased = vec![];
        for row in due {
            let (w, id): (String, String) = (row.try_get("", "workspace")?, row.try_get("", "id")?);
            match self.lease_check(&w, &id).await {
                Ok(Some(check)) => leased.push(check),
                Ok(None) => {}
                // One action's trouble doesn't hold up the others; it is tried next round.
                Err(_) => tracing::warn!("An unconfirmed write check could not be leased"),
            }
        }
        Ok(leased)
    }

    async fn lease_check(&self, w: &str, id: &str) -> Result<Option<Due>> {
        let tx = self.workspace_tx(w).await?;
        // A new check row, or the next attempt once its lease and time have passed.
        // Another controller's lease makes this return nothing.
        let Some(lease) = query(&tx, &format!("INSERT INTO relay_control_outcome_checks AS k(workspace,action,since,next_at,lease_until)
            SELECT a.workspace,a.id,{SINCE},{SINCE}+{first},{NOW}+{LEASE_SECONDS} FROM relay_control_actions a
            WHERE a.workspace=$1 AND a.id=$2 AND a.settled_at IS NULL AND {UNCONFIRMED} AND {SINCE}+{first}<={NOW}
            ON CONFLICT(workspace,action) DO UPDATE SET lease_until=EXCLUDED.lease_until
            WHERE k.lease_until<={NOW} AND k.attempts<3 AND k.next_at<={NOW}
            RETURNING k.attempts", first = AFTER[0]), vec![w.into(), id.into()]).await? else {
            return Ok(None);
        };
        let attempts: i32 = lease.try_get("", "attempts")?;
        let row = query(&tx, "SELECT a.operation,a.arguments,c.provider,c.server,c.binding,c.fingerprint,c.definition
            FROM relay_control_actions a JOIN relay_control_connections c ON c.workspace=a.workspace AND c.id=a.connection
            WHERE a.workspace=$1 AND a.id=$2", vec![w.into(), id.into()]).await?.context("Action unavailable")?;
        let operation: String = row.try_get("", "operation")?;
        let definition: Value = row.try_get("", "definition")?;
        let operation = definition["operations"]
            .as_array()
            .and_then(|ops| ops.iter().find(|op| op["id"] == operation))
            .and_then(|op| serde_json::from_value(op.clone()).ok());
        tx.commit().await?;
        Ok(Some(Due {
            workspace: w.into(),
            action: id.into(),
            attempts,
            provider: row.try_get("", "provider")?,
            server: row.try_get("", "server")?,
            binding: row.try_get("", "binding")?,
            fingerprint: row.try_get("", "fingerprint")?,
            operation,
            arguments: row.try_get("", "arguments")?,
        }))
    }

    /// Records what the provider said. `Happened` settles the action as completed
    /// (settled_as "checked", no person); anything else waits for a person. True
    /// when this check settled the action.
    pub(crate) async fn outcome_check_done(&self, due: &Due, asked: &Asked) -> Result<bool> {
        let tx = self.workspace_tx(&due.workspace).await?;
        let keys = vec![
            due.workspace.clone().into(),
            due.action.clone().into(),
            due.attempts.into(),
        ];
        let Asked::Found(found) = asked else {
            execute(&tx, "UPDATE relay_control_outcome_checks SET attempts=3,next_at=NULL,lease_until=0 WHERE workspace=$1 AND action=$2 AND attempts=$3", keys).await?;
            tx.commit().await?;
            return Ok(false);
        };
        let result = match found {
            Found::Happened(_) => "happened",
            Found::NotHappened => "not_happened",
            Found::Unknown => "unknown",
        };
        let mut values = keys;
        values.push(result.into());
        let recorded = execute(
            &tx,
            &format!(
                "UPDATE relay_control_outcome_checks SET attempts=attempts+1,lease_until=0,
            checks=checks||jsonb_build_array(jsonb_build_object('at',{NOW},'result',$4::text)),
            next_at=CASE WHEN attempts+1<3 THEN since+(ARRAY[{},{},{}])[attempts+2] END
            WHERE workspace=$1 AND action=$2 AND attempts=$3",
                AFTER[0], AFTER[1], AFTER[2]
            ),
            values,
        )
        .await?;
        let mut settled = false;
        if recorded == 1
            && let Found::Happened(value) = found
        {
            let mut outcome = json!({"message": super::CHECKED});
            if value.to_string().len() <= FOUND_LIMIT {
                outcome["found"] = value.clone();
            }
            settled = super::settle_row(
                &tx,
                &due.workspace,
                &due.action,
                None,
                "checked",
                None,
                outcome,
            )
            .await?;
        }
        if settled {
            let title = query(
                &tx,
                "SELECT title FROM relay_control_actions WHERE workspace=$1 AND id=$2",
                vec![due.workspace.clone().into(), due.action.clone().into()],
            )
            .await?
            .context("Action unavailable")?
            .try_get::<String>("", "title")?;
            audit_with_summary(
                &tx,
                "system",
                Some(&due.workspace),
                "control_action_settled",
                &due.action,
                &super::summary("found at the app that this happened:", &title),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(settled)
    }
}
