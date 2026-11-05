//! The Guard's company rules (core-agent CA1): one condition on one reviewed action,
//! checked inside the action's own transaction. A rule can only make an action
//! stricter (ask first or block); it never allows what the action rules block.
use super::*;
use crate::identity::control::model::Operation;
use serde::Deserialize;
use serde_json::json;

#[path = "guard_migration.rs"]
pub(in crate::identity::postgres) mod migration;
#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuardRule {
    pub id: String,
    pub operation: String,
    #[serde(default)]
    pub connection: Option<String>,
    pub kind: String,
    pub field: String,
    pub comparator: String,
    pub value: Value,
    pub effect: String,
    #[serde(default)]
    pub note: String,
    pub enabled: bool,
    pub revision: i64,
}

const VIEW: &str = "jsonb_build_object('id',g.id,'operation',g.operation,'connection',g.connection,'kind',g.kind,'field',g.field,'comparator',g.comparator,'value',g.value,'effect',g.effect,'note',g.note,'enabled',g.enabled,'revision',g.revision,'updated_ms',g.updated_ms,'updated_by_name',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=g.updated_by))";
/// States that use up a daily total: waiting approvals reserve their amount.
const COUNTED: &str = "('pending','executing','completed','uncertain')";
const MAX_RULES: i64 = 200;

/// The read-back people see, made by code from the rule itself, never by a model.
pub(crate) fn summary(rule: &Value, action: &str) -> String {
    let field = rule["field"].as_str().unwrap_or_default().replace('_', " ");
    let value = match &rule["value"] {
        Value::String(text) => format!("\"{text}\""),
        Value::Number(n) => n
            .as_f64()
            .map(|n| format!("{n}"))
            .unwrap_or_else(|| n.to_string()),
        other => other.to_string(),
    };
    let comparison = match rule["comparator"].as_str() {
        Some(">") => "is over",
        Some(">=") => "is at least",
        Some("<") => "is under",
        Some("<=") => "is at most",
        Some("!=") => "is not",
        _ => "is",
    };
    let effect = if rule["effect"] == "block" {
        "block it"
    } else {
        "ask first"
    };
    if rule["kind"] == "daily_total" {
        let reach = if rule["comparator"] == ">=" {
            "reach"
        } else {
            "go over"
        };
        format!("{action}: if today's total {field} would {reach} {value}, {effect}.")
    } else {
        format!("{action}: if {field} {comparison} {value}, {effect}.")
    }
}

/// `None` when the facts can't decide, for example a missing or wrongly typed field.
fn holds(actual: &Value, comparator: &str, expected: &Value) -> Option<bool> {
    if let (Some(a), Some(b)) = (actual.as_f64(), expected.as_f64()) {
        return Some(match comparator {
            ">" => a > b,
            ">=" => a >= b,
            "<" => a < b,
            "<=" => a <= b,
            "!=" => a != b,
            _ => a == b,
        });
    }
    let same_kind = std::mem::discriminant(actual) == std::mem::discriminant(expected);
    match comparator {
        "=" if same_kind => Some(actual == expected),
        "!=" if same_kind => Some(actual != expected),
        _ => None,
    }
}

/// The strictest company rule that applies to this exact request, as the decision
/// record saved on the action. Callers hold the workspace row lock (`workspace_tx`),
/// so two requests never both fit under the same daily total.
pub(super) async fn check(
    db: &impl ConnectionTrait,
    w: &str,
    connection: &str,
    operation: &Operation,
    arguments: &Value,
) -> Result<Option<Value>> {
    let rules = json_rows(db, "SELECT to_jsonb(g) AS data FROM relay_guard_rules g WHERE workspace=$1 AND operation=$2 AND enabled AND (connection IS NULL OR connection=$3) ORDER BY created_ms,id", vec![w.into(), operation.id.clone().into(), connection.into()]).await?;
    let mut strictest: Option<Value> = None;
    let mut matched = 0;
    for rule in rules {
        let field = rule["field"].as_str().unwrap();
        let comparator = rule["comparator"].as_str().unwrap();
        let actual = arguments.get(field).cloned().unwrap_or(Value::Null);
        let (decides, facts) = if rule["kind"] == "daily_total" {
            let before: f64 = query(db, &format!("SELECT COALESCE(sum((a.arguments->>$4)::numeric),0)::float8 AS total FROM relay_control_actions a WHERE a.workspace=$1 AND a.operation=$2 AND ($3::text IS NULL OR a.connection=$3) AND a.state IN {COUNTED} AND jsonb_typeof(a.arguments->$4)='number' AND a.created >= (SELECT floor(extract(epoch FROM date_trunc('day', clock_timestamp() AT TIME ZONE z) AT TIME ZONE z))::bigint FROM (SELECT COALESCE(timezone,'UTC') AS z FROM relay_workspaces WHERE id=$1) t)"),
                vec![w.into(), operation.id.clone().into(), rule["connection"].as_str().map(str::to_owned).into(), field.into()]).await?.context("Workspace unavailable")?.try_get("", "total")?;
            match actual.as_f64() {
                Some(amount) => (
                    holds(&json!(before + amount), comparator, &rule["value"]),
                    json!({"field":field,"amount":amount,"today_before":before,"today_after":before + amount,"limit":rule["value"]}),
                ),
                None => (
                    None,
                    json!({"field":field,"amount":actual,"limit":rule["value"]}),
                ),
            }
        } else {
            (
                holds(&actual, comparator, &rule["value"]),
                json!({"field":field,"value":actual}),
            )
        };
        // Unsure means hold: facts that can't decide ask a person, even for a block rule.
        let effect = match decides {
            Some(false) => continue,
            Some(true) => rule["effect"].as_str().unwrap(),
            None => "ask",
        };
        matched += 1;
        let stricter = strictest
            .as_ref()
            .is_none_or(|s| s["effect"] == "ask" && effect == "block");
        if stricter {
            strictest = Some(json!({
                "rule": rule["id"], "revision": rule["revision"], "effect": effect,
                "summary": summary(&rule, &operation.title), "facts": facts,
                "known": decides.is_some(),
            }));
        }
    }
    Ok(strictest.map(|mut s| {
        s["matched"] = json!(matched);
        s
    }))
}

/// The read-backs of the rules on one action, shown to the agent with its operations.
pub(super) async fn summaries(
    db: &impl ConnectionTrait,
    w: &str,
    connection: &str,
    operation: &str,
    title: &str,
) -> Result<Vec<String>> {
    let rules = json_rows(db, "SELECT to_jsonb(g) AS data FROM relay_guard_rules g WHERE workspace=$1 AND operation=$2 AND enabled AND (connection IS NULL OR connection=$3) ORDER BY created_ms,id", vec![w.into(), operation.into(), connection.into()]).await?;
    Ok(rules.iter().map(|r| summary(r, title)).collect())
}

/// The action's mode after the Guard: block wins, then ask, then the agent's rule.
pub(super) fn stricter(mode: &str, guard: Option<&Value>) -> String {
    match (mode, guard.and_then(|g| g["effect"].as_str())) {
        ("block", _) | (_, Some("block")) => "block",
        ("ask", _) | (_, Some("ask")) => "ask",
        _ => mode,
    }
    .into()
}

async fn action_title(
    db: &impl ConnectionTrait,
    w: &str,
    operation: &str,
    connection: Option<&str>,
) -> Result<Option<(String, Value)>> {
    let Some(row) = query(db, "SELECT op->>'title' AS title, op->'schema' AS schema FROM relay_control_connections c CROSS JOIN LATERAL jsonb_array_elements(c.definition->'operations') op WHERE c.workspace=$1 AND op->>'id'=$2 AND ($3::text IS NULL OR c.id=$3) ORDER BY c.enabled DESC,c.id LIMIT 1",
        vec![w.into(), operation.into(), connection.map(str::to_owned).into()]).await? else { return Ok(None) };
    Ok(Some((
        row.try_get("", "title")?,
        row.try_get("", "schema")?,
    )))
}

impl PgStore {
    /// Company rules, readable by every member; the actions they can apply to.
    pub(crate) async fn guard_rules(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let mut rules = json_rows(&tx, &format!("SELECT {VIEW} AS data FROM relay_guard_rules g WHERE g.workspace=$1 ORDER BY g.enabled DESC,g.created_ms,g.id"), vec![w.into()]).await?;
        for rule in &mut rules {
            let title = action_title(
                &tx,
                w,
                rule["operation"].as_str().unwrap(),
                rule["connection"].as_str(),
            )
            .await?
            .map(|t| t.0)
            .unwrap_or_else(|| rule["operation"].as_str().unwrap().to_owned());
            rule["summary"] = json!(summary(rule, &title));
        }
        let actions = json_rows(&tx, "SELECT jsonb_build_object('connection',c.id,'account',c.definition->>'name','operation',op->>'id','title',op->>'title','effect',op->>'effect','fields',(SELECT COALESCE(jsonb_agg(jsonb_build_object('name',f.key,'type',f.value->>'type') ORDER BY f.key),'[]'::jsonb) FROM jsonb_each(op->'schema'->'properties') f)) AS data FROM relay_control_connections c CROSS JOIN LATERAL jsonb_array_elements(c.definition->'operations') op WHERE c.workspace=$1 AND c.enabled ORDER BY c.definition->>'name',op->>'title'", vec![w.into()]).await?;
        tx.commit().await?;
        Ok(json!({"rules":rules,"actions":actions,"can_manage":admin}))
    }
    /// Creates or changes one rule. Admins only; the revision guards lost updates.
    pub(crate) async fn guard_rule_save(
        &self,
        user: &User,
        w: &str,
        input: &GuardRule,
    ) -> Result<Value> {
        crate::identity::worker_control::model::uuid(&input.id)?;
        ensure!(
            ["field", "daily_total"].contains(&input.kind.as_str()),
            "Unsupported rule kind"
        );
        ensure!(
            ["ask", "block"].contains(&input.effect.as_str()),
            "A rule can ask first or block"
        );
        ensure!(
            [">", ">=", "<", "<=", "=", "!="].contains(&input.comparator.as_str()),
            "Unsupported comparison"
        );
        ensure!(input.note.chars().count() <= 280, "The note is too long");
        let numeric = !["=", "!="].contains(&input.comparator.as_str());
        ensure!(
            input.kind == "field" || [">", ">="].contains(&input.comparator.as_str()),
            "A daily total can be over or reach a limit"
        );
        ensure!(
            input.value.is_number() || input.value.is_string() || input.value.is_boolean(),
            "Unsupported rule value"
        );
        ensure!(
            input
                .value
                .as_str()
                .is_none_or(|s| s.chars().count() <= 200),
            "The value is too long"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins can change company rules"
        );
        let (title, schema) = action_title(&tx, w, &input.operation, input.connection.as_deref())
            .await?
            .context("No connected account offers this action")?;
        let kind = schema["properties"][&input.field]["type"]
            .as_str()
            .context("This action has no such field")?
            .to_owned();
        let number = ["integer", "number"].contains(&kind.as_str());
        if numeric || input.kind == "daily_total" {
            ensure!(
                number && input.value.is_number(),
                "Compare amounts with a number field and a number"
            );
        } else {
            let matches = match kind.as_str() {
                "string" => input.value.is_string(),
                "boolean" => input.value.is_boolean(),
                _ => input.value.is_number(),
            };
            ensure!(matches, "The value must match the field's type");
        }
        let current: Option<i64> = query(
            &tx,
            "SELECT revision FROM relay_guard_rules WHERE workspace=$1 AND id=$2",
            vec![w.into(), input.id.clone().into()],
        )
        .await?
        .map(|r| r.try_get("", "revision"))
        .transpose()?;
        ensure!(
            current.unwrap_or(0) == input.revision,
            "This rule changed; reload and try again"
        );
        if current.is_none() {
            let count: i64 = query(
                &tx,
                "SELECT count(*) AS n FROM relay_guard_rules WHERE workspace=$1",
                vec![w.into()],
            )
            .await?
            .unwrap()
            .try_get("", "n")?;
            ensure!(
                count < MAX_RULES,
                "This workspace reached its company rule limit"
            );
        }
        execute(&tx, "INSERT INTO relay_guard_rules(workspace,id,operation,connection,kind,field,comparator,value,effect,note,enabled,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$12) ON CONFLICT(workspace,id) DO UPDATE SET operation=EXCLUDED.operation,connection=EXCLUDED.connection,kind=EXCLUDED.kind,field=EXCLUDED.field,comparator=EXCLUDED.comparator,value=EXCLUDED.value,effect=EXCLUDED.effect,note=EXCLUDED.note,enabled=EXCLUDED.enabled,updated_by=EXCLUDED.updated_by,revision=relay_guard_rules.revision+1,updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint",
            vec![w.into(), input.id.clone().into(), input.operation.clone().into(), input.connection.clone().into(), input.kind.clone().into(), input.field.clone().into(), input.comparator.clone().into(), input.value.clone().into(), input.effect.clone().into(), input.note.clone().into(), input.enabled.into(), user.id.clone().into()]).await?;
        let mut rule: Value = query(
            &tx,
            &format!(
                "SELECT {VIEW} AS data FROM relay_guard_rules g WHERE g.workspace=$1 AND g.id=$2"
            ),
            vec![w.into(), input.id.clone().into()],
        )
        .await?
        .unwrap()
        .try_get("", "data")?;
        rule["summary"] = json!(summary(&rule, &title));
        let said = format!(
            "{} company rule: {}",
            if input.enabled { "Saved" } else { "Turned off" },
            rule["summary"].as_str().unwrap()
        );
        audit_with_summary(&tx, &user.id, Some(w), "guard_rule_saved", &input.id, &said).await?;
        tx.commit().await?;
        Ok(rule)
    }
}
