//! Creating company rules safely (core-agent CA2): agents propose rules from a person's
//! words, admins replay a draft on the last 30 days before turning it on, and unreviewed
//! proposals can be dismissed. An agent can never turn a rule on or loosen one.
use super::super::*;
use super::store::{ACTIONS, GuardRule, VIEW, title_of, validate};
use super::{DAYS, fact_at, holds, in_window, summary};
use crate::identity::chat::model::Job;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

#[cfg(test)]
#[path = "guard_proposal_tests.rs"]
mod tests;

/// What an agent drafts from a person's words; the same shape as a rule.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuleProposal {
    pub words: String,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub connection: Option<String>,
    #[serde(default)]
    pub meaning: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub source: Option<Value>,
    #[serde(default)]
    pub field: String,
    pub comparator: String,
    pub value: Value,
    pub effect: String,
}
const MAX_PROPOSALS: i64 = 20;
const REPLAY_DAYS: i64 = 30;

impl PgStore {
    /// An agent drafts a rule for an admin to review. It stays off, and only covers
    /// accounts this agent uses.
    pub(crate) async fn propose_guard_rule(
        &self,
        job: &Job,
        input: &RuleProposal,
    ) -> Result<Value> {
        let words = input.words.trim();
        ensure!(
            !words.is_empty() && words.chars().count() <= 280,
            "Give the person's words in one line"
        );
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::authorize(&tx, job, true).await?;
        let actions: Vec<Value> =
            json_rows(&tx, ACTIONS, vec![job.workspace.clone().into()]).await?;
        let used: Vec<String> = rows(
            &tx,
            "SELECT connection FROM relay_agent_connections WHERE workspace=$1 AND agent=$2",
            vec![job.workspace.clone().into(), job.agent.clone().into()],
        )
        .await?
        .into_iter()
        .map(|r| r.try_get("", "connection"))
        .collect::<Result<_, _>>()?;
        let mine: Vec<Value> = actions
            .into_iter()
            .filter(|a| used.iter().any(|c| a["connection"] == c.as_str()))
            .collect();
        let rule = GuardRule {
            id: uuid::Uuid::new_v4().to_string(),
            operation: input.operation.clone(),
            connection: input.connection.clone(),
            meaning: input.meaning.clone(),
            kind: input.kind.clone(),
            source: input.source.clone(),
            field: input.field.clone(),
            comparator: input.comparator.clone(),
            value: input.value.clone(),
            effect: input.effect.clone(),
            note: String::new(),
            enabled: false,
            revision: 0,
            trial: false,
        };
        if let Err(error) = validate(&rule, &mine) {
            tx.commit().await?;
            return Ok(json!({"state":"invalid","message":error.to_string()}));
        }
        let waiting: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_guard_rules WHERE workspace=$1 AND NOT reviewed",
            vec![job.workspace.clone().into()],
        )
        .await?
        .unwrap()
        .try_get("", "n")?;
        ensure!(
            waiting < MAX_PROPOSALS,
            "Admins have too many proposed rules to review; ask them first"
        );
        let source = rule.source.clone().map(|mut source| {
            if let Some(title) = mine
                .iter()
                .find(|a| a["operation"] == source["operation"])
                .and_then(|a| a["title"].as_str())
            {
                source["title"] = json!(title);
            }
            source
        });
        execute(&tx, "INSERT INTO relay_guard_rules(workspace,id,operation,connection,meaning,kind,field,comparator,value,effect,enabled,created_by,updated_by,source,proposed_by,proposal,reviewed) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,false,$11,$11,$12,$13,$14,false)",
            vec![job.workspace.clone().into(), rule.id.clone().into(), rule.operation.clone().into(), rule.connection.clone().into(), rule.meaning.clone().into(), rule.kind.clone().into(), rule.field.clone().into(), rule.comparator.clone().into(), rule.value.clone().into(), rule.effect.clone().into(), job.requester.clone().into(), source.into(), job.agent.clone().into(), words.into()]).await?;
        let saved: Value = query(
            &tx,
            &format!(
                "SELECT {VIEW} AS data FROM relay_guard_rules g WHERE g.workspace=$1 AND g.id=$2"
            ),
            vec![job.workspace.clone().into(), rule.id.clone().into()],
        )
        .await?
        .unwrap()
        .try_get("", "data")?;
        let said = summary(&saved, &title_of(&saved, &mine));
        audit_with_summary(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "guard_rule_proposed",
            &rule.id,
            &format!("An agent proposed a company rule: {said}"),
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"state":"proposed","rule":rule.id,"summary":said,"next":"An admin reviews it in Settings, Company rules. It isn't in force until they turn it on. Tell the person in one line."}),
        )
    }

    /// An admin discards a proposal nobody turned on.
    pub(crate) async fn guard_rule_dismiss(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins can review proposed rules"
        );
        let words: String = query(&tx, "DELETE FROM relay_guard_rules WHERE workspace=$1 AND id=$2 AND NOT reviewed RETURNING proposal", vec![w.into(), id.into()]).await?
            .context("Only proposals that were never turned on can be dismissed")?.try_get("", "proposal")?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "guard_rule_dismissed",
            id,
            &format!("Dismissed a proposed company rule: \"{words}\""),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"dismissed":true}))
    }

    /// What a draft rule would have done to the last 30 days of requests. Counts use what
    /// really happened, so a hold in the replay doesn't change later totals.
    pub(crate) async fn guard_rule_replay(
        &self,
        user: &User,
        w: &str,
        input: &GuardRule,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins can try company rules"
        );
        let actions: Vec<Value> = json_rows(&tx, ACTIONS, vec![w.into()]).await?;
        validate(input, &actions)?;
        let rule = serde_json::to_value(
            json!({"kind":input.kind,"field":input.field,"comparator":input.comparator,"value":input.value,"effect":input.effect,"meaning":input.meaning,"source":input.source}),
        )?;
        let past = json_rows(&tx, &format!("SELECT jsonb_build_object('id',a.id,'job',a.job,'connection',a.connection,'operation',a.operation,'meaning',a.meaning,'arguments',a.arguments,'state',a.state,'title',a.title,'created',a.created,'effect',a.effect,'result',a.result,'day',to_char(to_timestamp(a.created) AT TIME ZONE z,'YYYY-MM-DD'),'hm',to_char(to_timestamp(a.created) AT TIME ZONE z,'HH24:MI'),'dow',extract(isodow FROM to_timestamp(a.created) AT TIME ZONE z)::int) AS data FROM relay_control_actions a, (SELECT COALESCE(timezone,'UTC') AS z FROM relay_workspaces WHERE id=$1) t WHERE a.workspace=$1 AND a.created >= floor(extract(epoch FROM clock_timestamp()))::bigint-{} AND a.state<>'invalidated' ORDER BY a.created,a.id LIMIT 5000", REPLAY_DAYS * 86400), vec![w.into()]).await?;
        tx.commit().await?;
        let schema_of = |a: &Value| {
            actions
                .iter()
                .find(|x| x["connection"] == a["connection"] && x["operation"] == a["operation"])
                .map(|x| x["fields"].clone())
                .unwrap_or(Value::Null)
        };
        let field = input.field.as_str();
        let (mut examined, mut hold, mut block, mut examples) = (0, 0, 0, vec![]);
        let mut totals: HashMap<String, f64> = HashMap::new();
        for (index, a) in past.iter().enumerate() {
            let targeted = match (&input.operation, &input.meaning) {
                (Some(op), _) => {
                    a["operation"] == op.as_str()
                        && input
                            .connection
                            .as_deref()
                            .is_none_or(|c| a["connection"] == c)
                }
                (_, Some(m)) => {
                    a["meaning"] == m.as_str()
                        && (input.kind == "time"
                            || input.source.is_some()
                            || schema_of(a)
                                .as_array()
                                .is_some_and(|f| f.iter().any(|f| f["name"] == field)))
                }
                _ => false,
            };
            let counted = ["pending", "executing", "completed", "uncertain"]
                .contains(&a["state"].as_str().unwrap_or(""));
            let read = input.source.as_ref().and_then(|source| {
                past[..index].iter().rev().find(|r| {
                    r["job"] == a["job"]
                        && r["connection"] == a["connection"]
                        && r["operation"] == source["operation"]
                        && r["effect"] == "read"
                        && r["state"] == "completed"
                        && !a["arguments"][source["match"].as_str().unwrap_or("")].is_null()
                        && r["arguments"][source["match"].as_str().unwrap_or("")]
                            == a["arguments"][source["match"].as_str().unwrap_or("")]
                })
            });
            let actual = match (&input.source, read) {
                (None, _) => a["arguments"].get(field).cloned().unwrap_or(Value::Null),
                (Some(_), Some(r)) => fact_at(&r["result"], field),
                _ => Value::Null,
            };
            let day = a["day"].as_str().unwrap_or("").to_owned();
            if targeted {
                examined += 1;
                let decides = match input.kind.as_str() {
                    "read_first" => Some(read.is_none()),
                    "time" => Some(in_window(
                        &input.value,
                        a["hm"].as_str().unwrap_or(""),
                        a["dow"].as_u64().unwrap_or(1),
                    )),
                    "daily_total" => actual.as_f64().and_then(|amount| {
                        holds(
                            &json!(totals.get(&day).copied().unwrap_or(0.0) + amount),
                            &input.comparator,
                            &input.value,
                        )
                    }),
                    _ => holds(&actual, &input.comparator, &input.value),
                };
                let effect = match decides {
                    Some(false) => None,
                    Some(true) => Some(input.effect.as_str()),
                    None => Some("ask"),
                };
                if let Some(effect) = effect {
                    if effect == "block" {
                        block += 1
                    } else {
                        hold += 1
                    }
                    if examples.len() < 10 {
                        examples.push(json!({"id":a["id"],"title":a["title"],"created":a["created"],"state":a["state"],"effect":effect,"arguments":a["arguments"],"day":DAYS[(a["dow"].as_u64().unwrap_or(1) as usize).clamp(1, 7) - 1]}));
                    }
                }
            }
            if counted && input.kind == "daily_total" && targeted {
                *totals.entry(day).or_default() += actual.as_f64().unwrap_or(0.0);
            }
        }
        Ok(
            json!({"days":REPLAY_DAYS,"examined":examined,"would_hold":hold,"would_block":block,"examples":examples,"summary":summary(&rule, &title_of(&serde_json::to_value(json!({"operation":input.operation,"connection":input.connection,"meaning":input.meaning}))?, &actions))}),
        )
    }
}
