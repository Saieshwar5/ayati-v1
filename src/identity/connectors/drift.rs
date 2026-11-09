//! The drift check: does every connected account's reviewed action still mean,
//! at its provider, what was reviewed when the account connected? Providers
//! answer through [`ConnectorProvider::describe`]; nothing is sent to an app.
//! It runs nightly on one controller (leased in PostgreSQL) and on demand with
//! `ayati admin connectors verify`; both save their results (schema 110), and
//! accounts whose actions changed show in Needs fixing for affected agents.
use super::{ConnectorProvider, Saved};
use crate::identity::control::model::Operation;
use crate::identity::postgres::PgStore;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

/// How often a controller looks whether tonight's check is due.
const EVERY: Duration = Duration::from_secs(600);
/// The nightly check runs after this hour, UTC.
const NIGHTLY_HOUR_UTC: i64 = 2;
/// Longest one provider may take to describe one action.
const DESCRIBE_SECONDS: u64 = 30;
const DAY_MS: i64 = 86_400_000;

/// What the check found for one reviewed action.
#[derive(Debug, PartialEq)]
pub(crate) struct Finding {
    /// `ok`, `missing`, `changed`, `new_version` or `failed` (not checked).
    pub status: &'static str,
    /// Cuckoo field names that differ: now (`changed`) or in the newer
    /// version (`new_version`).
    pub fields: Vec<String>,
    /// The provider's newer version (`new_version`).
    pub version: Option<String>,
}
impl Finding {
    fn plain(status: &'static str) -> Self {
        Self {
            status,
            fields: vec![],
            version: None,
        }
    }
    /// Missing and changed actions are refused at the next call; an action that
    /// couldn't be checked is unknown. A newer version alone is not a problem:
    /// calls keep the pinned one.
    pub(crate) fn problem(&self) -> bool {
        matches!(self.status, "missing" | "changed" | "failed")
    }
}

/// Compares a provider's `describe` answer with the saved, reviewed operation.
/// The answer is `{"version", "schema", "latest": {"version", "schema"} | null}`:
/// `schema` is the reviewed fields as the provider defines them now at the
/// pinned version (null when the action is gone); `latest` only for providers
/// that version their tools.
pub(crate) fn compare(reviewed: &Operation, described: &Value) -> Finding {
    let pinned = &described["schema"];
    if pinned.is_null() {
        return Finding::plain("missing");
    }
    if *pinned != reviewed.schema {
        return Finding {
            status: "changed",
            fields: differing(&reviewed.schema, pinned),
            version: None,
        };
    }
    let latest = &described["latest"];
    match latest["version"].as_str() {
        Some(version) if Some(version) != reviewed.version.as_deref() => Finding {
            status: "new_version",
            fields: differing(&reviewed.schema, &latest["schema"]),
            version: Some(version.to_owned()),
        },
        _ => Finding::plain("ok"),
    }
}

/// Field names whose definition or required-ness differs between two closed
/// schemas. `after` may be null: every field then differs.
fn differing(before: &Value, after: &Value) -> Vec<String> {
    let names = |schema: &Value| -> BTreeSet<String> {
        schema["properties"]
            .as_object()
            .map(|p| p.keys().cloned().collect())
            .unwrap_or_default()
    };
    let required = |schema: &Value, name: &str| {
        schema["required"]
            .as_array()
            .is_some_and(|r| r.iter().any(|v| v == name))
    };
    names(before)
        .union(&names(after))
        .filter(|name| {
            before["properties"].get(name.as_str()) != after["properties"].get(name.as_str())
                || required(before, name) != required(after, name)
        })
        .cloned()
        .collect()
}

/// Checks every connected account in one run and saves the results as it goes.
/// `lookup` finds a provider by name. Returns the report the CLI prints.
pub(crate) async fn check(
    store: &PgStore,
    run: &str,
    lookup: &(dyn Fn(&str) -> Result<Arc<dyn ConnectorProvider>> + Sync),
) -> Result<Value> {
    let (accounts, skipped) = store.connector_drift_accounts().await?;
    let mut report = vec![];
    let mut counts = json!({"ok":0,"missing":0,"changed":0,"new_version":0,"failed":0});
    let mut problems = 0;
    for account in &accounts {
        let provider = lookup(&account.provider).ok();
        let saved = Saved {
            workspace: &account.workspace,
            server: &account.server,
            binding: account.binding.as_ref(),
            fingerprint: &account.fingerprint,
        };
        let mut findings = vec![];
        for operation in &account.operations {
            let finding = match &provider {
                Some(provider) => describe(provider.as_ref(), &saved, operation).await,
                None => Finding::plain("failed"),
            };
            counts[finding.status] = json!(counts[finding.status].as_i64().unwrap_or(0) + 1);
            problems += usize::from(finding.problem());
            findings.push((operation, finding));
        }
        store.connector_drift_save(run, account, &findings).await?;
        report.push(json!({
            "workspace": account.workspace, "workspace_name": account.workspace_name,
            "connection": account.connection, "app": account.server, "name": account.name,
            "provider": account.provider,
            "actions": findings.iter().map(|(op, f)| json!({"id": op.id, "title": op.title,
                "status": f.status, "fields": f.fields, "version": f.version})).collect::<Vec<_>>(),
        }));
    }
    store
        .connector_drift_finish(run, accounts.len(), skipped)
        .await?;
    Ok(
        json!({"run": run, "accounts": report, "skipped": skipped, "counts": counts, "problems": problems}),
    )
}

async fn describe(provider: &dyn ConnectorProvider, saved: &Saved<'_>, op: &Operation) -> Finding {
    match tokio::time::timeout(
        Duration::from_secs(DESCRIBE_SECONDS),
        provider.describe(saved, op),
    )
    .await
    {
        Ok(Ok(described)) => compare(op, &described),
        // Provider errors are not logged: their answers can carry account details.
        _ => Finding::plain("failed"),
    }
}

/// When tonight's check is due: `NIGHTLY_HOUR_UTC` on the current UTC day,
/// or the day before when that hour hasn't come yet.
pub(crate) fn nightly_slot(now_ms: i64) -> i64 {
    let hour = NIGHTLY_HOUR_UTC * 3_600_000;
    (now_ms - hour).div_euclid(DAY_MS) * DAY_MS + hour
}

/// The nightly check on every controller; a lease lets only one run it.
pub(in crate::identity) fn start(app: crate::identity::Shared) {
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let mut tick = tokio::time::interval(EVERY);
        loop {
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tick.tick() => {}
            }
            let run = match app.store.connector_drift_claim().await {
                Ok(Some(run)) => run,
                Ok(None) => continue,
                Err(_) => {
                    tracing::warn!("The connector drift check is unavailable; it will retry");
                    continue;
                }
            };
            match check(&app.store, &run, &super::named).await {
                Ok(report) => tracing::info!(
                    accounts = report["accounts"].as_array().map_or(0, Vec::len),
                    problems = report["problems"].as_u64().unwrap_or(0),
                    "Checked connected accounts for provider changes"
                ),
                Err(_) => tracing::warn!("The connector drift check stopped; it will retry"),
            }
        }
    });
}

/// The connector providers `ayati admin` uses: the same settings as the
/// controller's, read from its environment.
pub(crate) fn admin_settings(config: &crate::config::Admin) -> anyhow::Result<super::Settings> {
    super::settings_from(&config.connectors, config.mcp_servers_file.clone())
}

/// `ayati admin connectors verify` for people: one line per action.
pub(crate) fn report_text(report: &Value) -> String {
    let text = |v: &Value| v.as_str().unwrap_or("—").to_owned();
    let mut out = String::new();
    for account in report["accounts"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "{} · {} ({}) · workspace {} ({})\n",
            text(&account["name"]),
            text(&account["app"]),
            text(&account["provider"]),
            text(&account["workspace_name"]),
            text(&account["workspace"]),
        ));
        for action in account["actions"].as_array().into_iter().flatten() {
            let fields: Vec<String> = action["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .map(text)
                .collect();
            let detail = match action["status"].as_str() {
                Some("ok") => "ok".to_owned(),
                Some("missing") => "missing at the provider".to_owned(),
                Some("changed") if fields.is_empty() => "changed".to_owned(),
                Some("changed") => format!("changed fields: {}", fields.join(", ")),
                Some("new_version") if fields.is_empty() => format!(
                    "new version {} (reviewed fields unchanged)",
                    text(&action["version"])
                ),
                Some("new_version") => format!(
                    "new version {} (fields differ: {})",
                    text(&action["version"]),
                    fields.join(", ")
                ),
                _ => "not checked: the provider did not answer".to_owned(),
            };
            out.push_str(&format!("  {:<32} {detail}\n", text(&action["id"])));
        }
    }
    let n = |key: &str| report["counts"][key].as_u64().unwrap_or(0);
    out.push_str(&format!(
        "{} accounts: {} ok, {} new version, {} changed, {} missing, {} not checked; {} skipped (needs reconnecting or provider off)\n",
        report["accounts"].as_array().map_or(0, Vec::len),
        n("ok"),
        n("new_version"),
        n("changed"),
        n("missing"),
        n("failed"),
        report["skipped"].as_u64().unwrap_or(0),
    ));
    out
}

#[cfg(test)]
#[path = "drift_tests.rs"]
mod tests;
