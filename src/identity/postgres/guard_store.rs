//! Saving company rules and action meanings (core-agent CA1). Admins only; every
//! member reads them. Revisions guard lost updates; every save is audited.
use super::super::*;
use super::{MEANINGS, meaning_label, summary};
use serde::Deserialize;
use serde_json::json;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuardRule {
    pub id: String,
    /// One action (with an optional account), or every action of `meaning`.
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub connection: Option<String>,
    #[serde(default)]
    pub meaning: Option<String>,
    pub kind: String,
    /// A value from an earlier read in the same task: `{operation, match, path?}`.
    #[serde(default)]
    pub source: Option<Value>,
    #[serde(default)]
    pub field: String,
    pub comparator: String,
    pub value: Value,
    pub effect: String,
    #[serde(default)]
    pub note: String,
    pub enabled: bool,
    pub revision: i64,
    /// Turning on with a trial week: it records what it would do, then enforces.
    #[serde(default)]
    pub trial: bool,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActionMeaning {
    pub connection: String,
    pub operation: String,
    pub meaning: String,
    pub revision: i64,
}

pub(super) const VIEW: &str = "jsonb_build_object('id',g.id,'operation',g.operation,'connection',g.connection,'meaning',g.meaning,'source',g.source,'kind',g.kind,'field',g.field,'comparator',g.comparator,'value',g.value,'effect',g.effect,'note',g.note,'enabled',g.enabled,'revision',g.revision,'updated_ms',g.updated_ms,'updated_by_name',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=g.updated_by),'proposal',g.proposal,'reviewed',g.reviewed,'proposed_by_name',(SELECT s.name FROM relay_shared_agents s WHERE s.workspace=g.workspace AND s.id=g.proposed_by),'trial_until_ms',g.trial_until_ms,'trial_matches',(SELECT count(*) FROM relay_control_actions a WHERE a.workspace=g.workspace AND g.trial_until_ms IS NOT NULL AND a.guard->'trial' @> jsonb_build_array(jsonb_build_object('rule',g.id))))";
const MAX_RULES: i64 = 200;
/// Every connected action with its meaning and fields, for the rule editor.
pub(super) const ACTIONS: &str = "SELECT jsonb_build_object('connection',c.id,'account',c.definition->>'name','operation',op->>'id','title',op->>'title','effect',op->>'effect','meaning',relay_action_meaning(c.workspace,c.id,op),'fields',(SELECT COALESCE(jsonb_agg(jsonb_build_object('name',f.key,'type',f.value->>'type') ORDER BY f.key),'[]'::jsonb) FROM jsonb_each(op->'schema'->'properties') f)) AS data FROM relay_control_connections c CROSS JOIN LATERAL jsonb_array_elements(c.definition->'operations') op WHERE c.workspace=$1 AND c.enabled ORDER BY c.definition->>'name',op->>'title'";

/// The title a rule's read-back uses: the action's title, or its meaning's label.
pub(super) fn title_of(rule: &Value, actions: &[Value]) -> String {
    if let Some(meaning) = rule["meaning"].as_str() {
        return meaning_label(meaning).to_owned();
    }
    actions
        .iter()
        .find(|a| {
            a["operation"] == rule["operation"]
                && (rule["connection"].is_null() || a["connection"] == rule["connection"])
        })
        .and_then(|a| a["title"].as_str())
        .or(rule["operation"].as_str())
        .unwrap_or_default()
        .to_owned()
}

fn clock(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        v.len() == 5
            && v.as_bytes()[2] == b':'
            && v[..2].parse::<u8>().is_ok_and(|h| h < 24)
            && v[3..].parse::<u8>().is_ok_and(|m| m < 60)
    })
}

/// Checks a rule's shape against the actions it targets; returns their field type.
pub(super) fn validate(input: &GuardRule, actions: &[Value]) -> Result<()> {
    crate::identity::worker_control::model::uuid(&input.id)?;
    ensure!(
        ["field", "daily_total", "time", "read_first"].contains(&input.kind.as_str()),
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
    ensure!(
        input.operation.is_some() != input.meaning.is_some(),
        "A rule is for one action or for every action of one kind"
    );
    ensure!(
        input.connection.is_none() || input.operation.is_some(),
        "An account needs an action"
    );
    ensure!(
        input
            .meaning
            .as_deref()
            .is_none_or(|m| MEANINGS.contains(&m)),
        "Unsupported kind of action"
    );
    let targets: Vec<&Value> = actions
        .iter()
        .filter(|a| match (&input.operation, &input.meaning) {
            (Some(op), _) => {
                a["operation"] == op.as_str()
                    && input
                        .connection
                        .as_deref()
                        .is_none_or(|c| a["connection"] == c)
            }
            (_, Some(m)) => a["meaning"] == m.as_str(),
            _ => false,
        })
        .collect();
    ensure!(
        !targets.is_empty(),
        "No connected account offers this action"
    );
    if let Some(source) = &input.source {
        return validate_source(input, source, &targets, actions);
    }
    ensure!(
        input.kind != "read_first",
        "A read-first rule needs the reading action"
    );
    if input.kind == "time" {
        let window = input
            .value
            .as_object()
            .context("A time rule needs a window")?;
        ensure!(
            window
                .keys()
                .all(|k| ["from", "to", "days"].contains(&k.as_str())),
            "Unsupported time window"
        );
        ensure!(
            clock(window.get("from").and_then(Value::as_str))
                && clock(window.get("to").and_then(Value::as_str)),
            "Times are HH:MM"
        );
        let days = window
            .get("days")
            .and_then(Value::as_array)
            .context("Choose the days, or none for every day")?;
        ensure!(
            days.len() <= 7
                && days
                    .iter()
                    .all(|d| d.as_u64().is_some_and(|d| (1..=7).contains(&d))),
            "Days are 1 (Monday) to 7 (Sunday)"
        );
        ensure!(
            input.field.is_empty() && input.comparator == "=",
            "A time rule has no field"
        );
        return Ok(());
    }
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
    let kinds: Vec<&str> = targets
        .iter()
        .filter_map(|a| {
            a["fields"]
                .as_array()?
                .iter()
                .find(|f| f["name"] == input.field.as_str())?["type"]
                .as_str()
        })
        .collect();
    ensure!(!kinds.is_empty(), "This action has no such field");
    let numeric = !["=", "!="].contains(&input.comparator.as_str()) || input.kind == "daily_total";
    ensure!(
        input.kind == "field" || [">", ">="].contains(&input.comparator.as_str()),
        "A daily total can be over or reach a limit"
    );
    if numeric {
        ensure!(
            input.value.is_number() && kinds.iter().any(|k| ["integer", "number"].contains(k)),
            "Compare amounts with a number field and a number"
        );
    } else {
        ensure!(
            kinds.iter().any(|k| match *k {
                "string" => input.value.is_string(),
                "boolean" => input.value.is_boolean(),
                _ => input.value.is_number(),
            }),
            "The value must match the field's type"
        );
    }
    Ok(())
}

/// A rule that takes its value from an earlier read, or needs that read first.
fn validate_source(
    input: &GuardRule,
    source: &Value,
    targets: &[&Value],
    actions: &[Value],
) -> Result<()> {
    let source = source.as_object().context("Unsupported source")?;
    ensure!(
        input.operation.is_some(),
        "A value from a read needs one action"
    );
    ensure!(
        source
            .keys()
            .all(|k| ["operation", "match", "path", "title"].contains(&k.as_str())),
        "Unsupported source"
    );
    let read = source
        .get("operation")
        .and_then(Value::as_str)
        .context("Choose the reading action")?;
    let key = source
        .get("match")
        .and_then(Value::as_str)
        .context("Choose the field both share")?;
    let has = |x: &Value| {
        x["fields"]
            .as_array()
            .is_some_and(|f| f.iter().any(|f| f["name"] == key))
    };
    let paired = targets.iter().any(|t| {
        has(t)
            && actions.iter().any(|a| {
                a["connection"] == t["connection"]
                    && a["operation"] == read
                    && a["effect"] == "read"
                    && has(a)
            })
    });
    ensure!(
        paired,
        "The reading action must be on the same account and share the matched field"
    );
    if input.kind == "read_first" {
        ensure!(
            input.field.is_empty()
                && input.comparator == "="
                && input.value == Value::Bool(true)
                && !source.contains_key("path"),
            "A read-first rule has no value"
        );
        return Ok(());
    }
    ensure!(
        ["field", "daily_total"].contains(&input.kind.as_str()),
        "This kind of rule can't use a read"
    );
    let path = source
        .get("path")
        .and_then(Value::as_str)
        .context("Choose the value in the read's result")?;
    ensure!(path == input.field && path.len() <= 80 && path.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')), "The result field is a dotted name");
    let numeric = !["=", "!="].contains(&input.comparator.as_str()) || input.kind == "daily_total";
    ensure!(
        input.kind == "field" || [">", ">="].contains(&input.comparator.as_str()),
        "A daily total can be over or reach a limit"
    );
    ensure!(
        !numeric || input.value.is_number(),
        "Compare amounts with a number"
    );
    ensure!(
        input.value.is_number() || input.value.is_string() || input.value.is_boolean(),
        "Unsupported rule value"
    );
    Ok(())
}

impl PgStore {
    /// Company rules, readable by every member; the actions they can apply to.
    pub(crate) async fn guard_rules(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let actions = json_rows(&tx, ACTIONS, vec![w.into()]).await?;
        let mut rules = json_rows(&tx, &format!("SELECT {VIEW} AS data FROM relay_guard_rules g WHERE g.workspace=$1 ORDER BY g.enabled DESC,g.created_ms,g.id"), vec![w.into()]).await?;
        for rule in &mut rules {
            rule["summary"] = json!(summary(rule, &title_of(rule, &actions)));
        }
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
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins can change company rules"
        );
        let actions = json_rows(&tx, ACTIONS, vec![w.into()]).await?;
        validate(input, &actions)?;
        // The read's title goes with the source, so the read-back can name it.
        let source = input.source.clone().map(|mut source| {
            let read = source["operation"].as_str().unwrap_or_default().to_owned();
            if let Some(title) = actions
                .iter()
                .find(|a| a["operation"] == read.as_str())
                .and_then(|a| a["title"].as_str())
            {
                source["title"] = json!(title);
            }
            source
        });
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
        execute(&tx, "INSERT INTO relay_guard_rules(workspace,id,operation,connection,meaning,kind,field,comparator,value,effect,note,enabled,created_by,updated_by,source,trial_until_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$13,$14,CASE WHEN $15 THEN (extract(epoch FROM clock_timestamp())*1000)::bigint+604800000 END) ON CONFLICT(workspace,id) DO UPDATE SET source=EXCLUDED.source,reviewed=true,trial_until_ms=CASE WHEN $15 THEN EXCLUDED.trial_until_ms WHEN NOT EXCLUDED.enabled THEN NULL ELSE relay_guard_rules.trial_until_ms END,operation=EXCLUDED.operation,connection=EXCLUDED.connection,meaning=EXCLUDED.meaning,kind=EXCLUDED.kind,field=EXCLUDED.field,comparator=EXCLUDED.comparator,value=EXCLUDED.value,effect=EXCLUDED.effect,note=EXCLUDED.note,enabled=EXCLUDED.enabled,updated_by=EXCLUDED.updated_by,revision=relay_guard_rules.revision+1,updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint",
            vec![w.into(), input.id.clone().into(), input.operation.clone().into(), input.connection.clone().into(), input.meaning.clone().into(), input.kind.clone().into(), input.field.clone().into(), input.comparator.clone().into(), input.value.clone().into(), input.effect.clone().into(), input.note.clone().into(), input.enabled.into(), user.id.clone().into(), source.into(), (input.trial && input.enabled).into()]).await?;
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
        rule["summary"] = json!(summary(&rule, &title_of(&rule, &actions)));
        let said = format!(
            "{} company rule: {}",
            if input.enabled { "Saved" } else { "Turned off" },
            rule["summary"].as_str().unwrap()
        );
        audit_with_summary(&tx, &user.id, Some(w), "guard_rule_saved", &input.id, &said).await?;
        tx.commit().await?;
        Ok(rule)
    }
    /// An admin confirms or corrects what one action does. Reading actions stay
    /// "read"; changing ones can't be called reading. Waiting approvals ask again.
    pub(crate) async fn action_meaning_save(
        &self,
        user: &User,
        w: &str,
        input: &ActionMeaning,
    ) -> Result<Value> {
        ensure!(
            MEANINGS.contains(&input.meaning.as_str()),
            "Unsupported kind of action"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins can review what actions do"
        );
        let row = query(&tx, "SELECT op->>'effect' AS effect, op->>'title' AS title FROM relay_control_connections c CROSS JOIN LATERAL jsonb_array_elements(c.definition->'operations') op WHERE c.workspace=$1 AND c.id=$2 AND op->>'id'=$3", vec![w.into(), input.connection.clone().into(), input.operation.clone().into()]).await?.context("Unsupported operation")?;
        let effect: String = row.try_get("", "effect")?;
        ensure!(
            (effect == "read") == (input.meaning == "read"),
            "Only reading actions are reading; changes need another kind"
        );
        let current: i64 = query(&tx, "SELECT revision FROM relay_action_meanings WHERE workspace=$1 AND connection=$2 AND operation=$3", vec![w.into(), input.connection.clone().into(), input.operation.clone().into()]).await?
            .map(|r| r.try_get("", "revision")).transpose()?.unwrap_or(0);
        ensure!(
            current == input.revision,
            "This action changed; reload and try again"
        );
        execute(&tx, "INSERT INTO relay_action_meanings(workspace,connection,operation,meaning,updated_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT(workspace,connection,operation) DO UPDATE SET meaning=EXCLUDED.meaning,updated_by=EXCLUDED.updated_by,revision=relay_action_meanings.revision+1,updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint",
            vec![w.into(), input.connection.clone().into(), input.operation.clone().into(), input.meaning.clone().into(), user.id.clone().into()]).await?;
        crate::identity::postgres::control_rules::invalidate(
            &tx,
            w,
            &input.connection,
            None,
            Some(&input.operation),
        )
        .await?;
        let title: String = row.try_get("", "title")?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "action_meaning_saved",
            &input.operation,
            &format!(
                "Reviewed {title} as {}",
                meaning_label(&input.meaning)
                    .trim_start_matches("Any ")
                    .to_lowercase()
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"saved":true,"revision":current + 1}))
    }
}
