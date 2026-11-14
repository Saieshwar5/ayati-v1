//! Results back to a connected system (C4 of the connected systems plan,
//! D249). Software can't rely on sentences, so an automation may name up to
//! eight result fields the agent must fill with `report_result`. The system
//! reads each run's state and result, or stops runs that haven't finished,
//! with its events key. (Cuckoo calling the system back comes later.)
use crate::identity::chat::model::Job;
use crate::identity::postgres::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// One named, typed value the agent must report.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResultField {
    pub name: String,
    /// `text`, `number`, `yes_no` or `date` (YYYY-MM-DD).
    pub kind: String,
}

pub(crate) fn check_fields(fields: &[ResultField]) -> Result<()> {
    ensure!(fields.len() <= 8, "Name at most 8 result fields");
    let mut seen = std::collections::HashSet::new();
    for field in fields {
        crate::identity::identifier(&field.name)
            .map_err(|_| anyhow::anyhow!("Result field names use letters, digits, - and _"))?;
        ensure!(seen.insert(&field.name), "A result field is named twice");
        ensure!(
            ["text", "number", "yes_no", "date"].contains(&field.kind.as_str()),
            "A result field is text, number, yes/no or date"
        );
    }
    Ok(())
}

/// Whether `value` is a `kind` value.
fn fits(kind: &str, value: &Value) -> bool {
    match kind {
        "text" => value.as_str().is_some_and(|s| s.len() <= 2000),
        "number" => value.as_f64().is_some_and(f64::is_finite),
        "yes_no" => value.is_boolean(),
        "date" => value.as_str().is_some_and(|s| {
            let b = s.as_bytes();
            b.len() == 10
                && b[4] == b'-'
                && b[7] == b'-'
                && b.iter()
                    .enumerate()
                    .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        }),
        _ => false,
    }
}

/// Checks a report against the automation's fields: every field, nothing else.
pub(crate) fn check_report(
    spec: &[ResultField],
    report: &Value,
) -> std::result::Result<(), String> {
    let object = report
        .as_object()
        .ok_or_else(|| "Report the fields as one object.".to_owned())?;
    for field in spec {
        match object.get(&field.name) {
            None => return Err(format!("Missing \u{201c}{}\u{201d}.", field.name)),
            Some(value) if !fits(&field.kind, value) => {
                return Err(format!(
                    "\u{201c}{}\u{201d} must be {}.",
                    field.name,
                    match field.kind.as_str() {
                        "number" => "a number",
                        "yes_no" => "true or false",
                        "date" => "a date like 2026-11-13",
                        _ => "text of up to 2,000 characters",
                    }
                ));
            }
            Some(_) => {}
        }
    }
    if let Some(extra) = object.keys().find(|k| spec.iter().all(|f| &f.name != *k)) {
        return Err(format!(
            "\u{201c}{extra}\u{201d} isn't one of the result fields."
        ));
    }
    Ok(())
}

/// The result fields of the automation a run came from, if any.
async fn spec(db: &impl ConnectionTrait, w: &str, reply: &str) -> Result<Option<Vec<ResultField>>> {
    if !super::events::ready(db).await? {
        return Ok(None);
    }
    let row = query(db, "SELECT a.results FROM relay_event_deliveries d JOIN relay_system_automations a ON a.workspace=d.workspace AND a.trigger_id=d.trigger_id WHERE d.workspace=$1 AND d.reply=$2",
        vec![w.into(), reply.into()]).await?;
    row.map(|r| Ok(serde_json::from_value(r.try_get("", "results")?)?))
        .transpose()
}

/// What a run's request adds when its automation has result fields.
pub(crate) async fn instruction(
    db: &impl ConnectionTrait,
    w: &str,
    trigger: &str,
) -> Result<String> {
    if !super::events::ready(db).await? {
        return Ok(String::new());
    }
    let row = query(
        db,
        "SELECT results FROM relay_system_automations WHERE workspace=$1 AND trigger_id=$2",
        vec![w.into(), trigger.into()],
    )
    .await?;
    let Some(row) = row else {
        return Ok(String::new());
    };
    let fields: Vec<ResultField> = serde_json::from_value(row.try_get("", "results")?)?;
    if fields.is_empty() {
        return Ok(String::new());
    }
    let listed: Vec<String> = fields
        .iter()
        .map(|f| format!("{} ({})", f.name, f.kind.replace('_', "/")))
        .collect();
    Ok(format!(
        "\n\nWhen you have finished, call report_result once with these fields: {}.",
        listed.join(", ")
    ))
}

/// The state a system sees for one run.
fn state(delivery: &str, job: Option<&str>, message: Option<&str>) -> &'static str {
    match (delivery, job, message) {
        ("cancelled", _, _) => "cancelled",
        (_, None, _) => "queued",
        (_, Some("queued"), _) => "queued",
        (_, Some("running"), _) => "working",
        (_, Some("waiting"), _) => "waiting_for_person",
        (_, _, Some("complete")) => "done",
        (_, _, Some("cancelled")) => "cancelled",
        _ => "failed",
    }
}

impl PgStore {
    /// `report_result`: the run's result fields, checked before they are kept.
    pub(crate) async fn report_result(&self, job: &Job, fields: &Value) -> Result<Value> {
        let Some(spec) = spec(&self.db, &job.workspace, &job.reply).await? else {
            return Ok(
                json!({"state":"denied","message":"This request has no result fields to report."}),
            );
        };
        if let Err(reason) = check_report(&spec, fields) {
            return Ok(json!({"state":"invalid","message":reason}));
        }
        execute(&self.db, "INSERT INTO relay_system_results(workspace,reply,fields) VALUES($1,$2,$3) ON CONFLICT(workspace,reply) DO UPDATE SET fields=excluded.fields,at_ms=excluded.at_ms",
            vec![job.workspace.clone().into(), job.reply.clone().into(), fields.clone().into()]).await?;
        Ok(json!({"state":"completed","message":"Result saved."}))
    }

    /// `GET /hooks/system/{connection}/events/{id}`: the event's runs, one per
    /// automation, with their state, written answer and result fields.
    pub(crate) async fn system_event_runs(
        &self,
        connection: &str,
        key: &str,
        id: &str,
    ) -> Result<Value> {
        let w = self.system_for_key(connection, key).await?;
        let runs = json_rows(&self.db, "SELECT jsonb_build_object('automation',t.input->>'name','delivery',d.state,'job',j.state,'message',m.state,
                'answer',CASE WHEN m.state='complete' THEN m.content END,'reason',d.detail,'reply',d.reply,
                'result',(SELECT r.fields FROM relay_system_results r WHERE r.workspace=d.workspace AND r.reply=d.reply),
                'results',(SELECT a.results FROM relay_system_automations a WHERE a.workspace=d.workspace AND a.trigger_id=d.trigger_id)) AS data
            FROM relay_event_deliveries d JOIN relay_connector_subscriptions s ON s.workspace=d.workspace AND s.id=d.source
            JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id
            LEFT JOIN relay_chat_jobs j ON j.workspace=d.workspace AND j.reply=d.reply
            LEFT JOIN relay_chat_messages m ON m.workspace=d.workspace AND m.id=d.reply
            WHERE d.workspace=$1 AND s.connection=$2 AND s.provider='system' AND d.event_id=$3 AND NOT d.revoked ORDER BY t.created_ms,t.id",
            vec![w.clone().into(), connection.into(), id.into()]).await?;
        let known = query(&self.db, "SELECT 1 AS found FROM relay_system_events WHERE workspace=$1 AND connection=$2 AND id=$3",
            vec![w.into(), connection.into(), id.into()]).await?.is_some();
        ensure!(known, "No event with this id was received");
        let runs: Vec<Value> = runs
            .into_iter()
            .map(|run| {
                let mut state = state(
                    run["delivery"].as_str().unwrap_or_default(),
                    run["job"].as_str(),
                    run["message"].as_str(),
                );
                let spec: Vec<ResultField> = serde_json::from_value(run["results"].clone()).unwrap_or_default();
                let mut reason = (!run["reason"].as_str().unwrap_or_default().is_empty()).then(|| run["reason"].clone());
                if state == "done" && !spec.is_empty() && check_report(&spec, &run["result"]).is_err() {
                    state = "failed";
                    reason = Some(json!("The agent finished without reporting the result fields."));
                }
                json!({"automation":run["automation"],"state":state,"answer":run["answer"],
                    "result": if spec.is_empty() { Value::Null } else { run["result"].clone() },"reason":reason})
            })
            .collect();
        Ok(json!({"id":id,"runs":runs}))
    }

    /// `POST /hooks/system/{connection}/events/{id}/cancel`: stops the event's
    /// runs that haven't finished. Finished runs keep their result.
    pub(crate) async fn cancel_system_event(
        &self,
        connection: &str,
        key: &str,
        id: &str,
    ) -> Result<Value> {
        let w = self.system_for_key(connection, key).await?;
        let tx = self.workspace_tx(&w).await?;
        let pending = execute(&tx, "UPDATE relay_event_deliveries d SET state='cancelled',detail='Cancelled by the system that sent the event.' FROM relay_connector_subscriptions s
            WHERE s.workspace=d.workspace AND s.id=d.source AND d.workspace=$1 AND s.connection=$2 AND d.event_id=$3 AND d.state='pending'",
            vec![w.clone().into(), connection.into(), id.into()]).await?;
        let running = rows(&tx, "SELECT j.payload FROM relay_event_deliveries d JOIN relay_connector_subscriptions s ON s.workspace=d.workspace AND s.id=d.source
            JOIN relay_chat_jobs j ON j.workspace=d.workspace AND j.reply=d.reply
            WHERE d.workspace=$1 AND s.connection=$2 AND d.event_id=$3 AND j.state<>'finished'",
            vec![w.clone().into(), connection.into(), id.into()]).await?;
        tx.commit().await?;
        let mut stopped = pending as usize;
        for row in running {
            let job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
            let person = User {
                id: job.requester.clone(),
                email: String::new(),
                display_name: None,
            };
            if self
                .cancel_assignment(&person, &w, &job.chat, &job.reply)
                .await
                .is_ok()
            {
                stopped += 1;
            }
        }
        Ok(json!({"id":id,"stopped":stopped}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec() -> Vec<ResultField> {
        vec![
            ResultField {
                name: "plan_saved".into(),
                kind: "yes_no".into(),
            },
            ResultField {
                name: "weak_subject".into(),
                kind: "text".into(),
            },
            ResultField {
                name: "next_exam".into(),
                kind: "date".into(),
            },
        ]
    }
    #[test]
    fn reports_must_fill_every_field_with_its_kind_and_nothing_else() {
        let ok = json!({"plan_saved":true,"weak_subject":"Maths","next_exam":"2026-12-01"});
        assert!(check_report(&spec(), &ok).is_ok());
        for (bad, says) in [
            (
                json!({"plan_saved":true,"weak_subject":"Maths"}),
                "next_exam",
            ),
            (
                json!({"plan_saved":"yes","weak_subject":"Maths","next_exam":"2026-12-01"}),
                "plan_saved",
            ),
            (
                json!({"plan_saved":true,"weak_subject":"Maths","next_exam":"1 Dec"}),
                "next_exam",
            ),
            (
                json!({"plan_saved":true,"weak_subject":"Maths","next_exam":"2026-12-01","extra":1}),
                "extra",
            ),
        ] {
            assert!(
                check_report(&spec(), &bad).unwrap_err().contains(says),
                "{bad}"
            );
        }
        assert!(check_fields(&spec()).is_ok());
        assert!(
            check_fields(&[ResultField {
                name: "x".into(),
                kind: "list".into()
            }])
            .is_err()
        );
    }
    #[test]
    fn a_system_sees_plain_run_states() {
        assert_eq!(state("pending", None, None), "queued");
        assert_eq!(
            state("accepted", Some("running"), Some("queued")),
            "working"
        );
        assert_eq!(
            state("accepted", Some("waiting"), Some("queued")),
            "waiting_for_person"
        );
        assert_eq!(
            state("accepted", Some("finished"), Some("complete")),
            "done"
        );
        assert_eq!(
            state("accepted", Some("finished"), Some("cancelled")),
            "cancelled"
        );
        assert_eq!(
            state("accepted", Some("finished"), Some("failed")),
            "failed"
        );
        assert_eq!(state("cancelled", None, None), "cancelled");
    }
}
