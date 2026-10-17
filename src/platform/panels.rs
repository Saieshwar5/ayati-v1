use super::{ExecutionRun, Platform};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDraft {
    pub id: String,
    pub expected_revision: i64,
    pub title: String,
    pub purpose: String,
    pub sections: Vec<Section>,
    pub sources: Vec<String>,
    #[serde(default)]
    pub publish: bool,
    #[serde(default)]
    pub updates_enabled: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Section {
    Text {
        heading: String,
        body: String,
    },
    Metric {
        label: String,
        value: String,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Chart {
        title: String,
        points: Vec<Point>,
    },
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub label: String,
    pub value: f64,
}
struct SavedPanel {
    id: String,
    agent: String,
    revision: i64,
    active: Option<i64>,
    definition: String,
    sources: String,
    task: String,
    date: String,
    updates: bool,
}
impl PanelDraft {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.updates_enabled || self.publish,
            "An update policy must be approved with publication"
        );
        ensure!(
            uuid::Uuid::parse_str(&self.id).is_ok_and(|id| id.to_string() == self.id),
            "Panel ID must be a canonical UUID"
        );
        ensure!(
            self.expected_revision >= 0
                && !self.title.trim().is_empty()
                && self.title.len() <= 100
                && self.purpose.len() <= 1000,
            "Panel title/purpose or revision is invalid"
        );
        ensure!(
            !self.sections.is_empty()
                && self.sections.len() <= 16
                && self.sources.len() <= 16
                && serde_json::to_vec(self)?.len() <= 12000,
            "Panel exceeds the saved view budget"
        );
        for section in &self.sections {
            match section {
                Section::Text { heading, body } => ensure!(
                    heading.len() <= 200 && body.len() <= 8000,
                    "Panel text exceeds the view budget"
                ),
                Section::Metric { label, value } => ensure!(
                    label.len() <= 200 && value.len() <= 500,
                    "Panel metric exceeds the view budget"
                ),
                Section::Table { columns, rows } => ensure!(
                    !columns.is_empty()
                        && columns.len() <= 12
                        && rows.len() <= 100
                        && rows.iter().all(|r| r.len() == columns.len()),
                    "Panel table must have 1–12 columns and at most 100 matching rows"
                ),
                Section::Chart { title, points } => ensure!(
                    title.len() <= 200
                        && !points.is_empty()
                        && points.len() <= 100
                        && points
                            .iter()
                            .all(|p| p.value.is_finite() && p.label.len() <= 100),
                    "Panel chart exceeds the supported point budget"
                ),
            }
        }
        Ok(())
    }
    fn definition(&self) -> Value {
        json!({"title":self.title,"purpose":self.purpose,"sections":self.sections})
    }
}
impl Platform {
    pub fn automatic_panel_update(&self, agent: &str, input: &Value) -> Result<bool> {
        let input: PanelDraft = serde_json::from_value(input.clone())?;
        input.validate()?;
        if input.publish {
            return Ok(false);
        }
        let previous:Option<(String,String)>=self.database()?.query_row("SELECT v.definition,v.sources FROM panels p JOIN panel_versions v ON v.panel_id=p.id AND v.revision=p.active_revision WHERE p.id=?1 AND p.agent_id=?2 AND p.state='saved' AND p.updates_enabled=1 AND p.active_revision=p.revision AND p.revision=?3",params![input.id,agent,input.expected_revision],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let Some((definition, sources)) = previous else {
            return Ok(false);
        };
        let definition: Value = serde_json::from_str(&definition)?;
        if definition["title"] != input.title || definition["purpose"] != input.purpose {
            return Ok(false);
        }
        let original: Vec<String> = serde_json::from_str(&sources)?;
        Ok(self.panel_source_grants(agent, &original)?
            == self.panel_source_grants(agent, &input.sources)?)
    }
    pub(super) fn panel_source_grants(
        &self,
        agent: &str,
        sources: &[String],
    ) -> Result<Vec<(String, String)>> {
        let db = self.database()?;
        let mut scope = Vec::new();
        for source in sources {
            let pair:Option<(String,String)>=db.query_row("SELECT e.connection_id,e.tool FROM executions e JOIN connections c ON c.id=e.connection_id JOIN grants g ON g.connection_id=e.connection_id AND g.tool=e.tool AND g.agent_id=e.agent_id WHERE e.id=?1 AND e.agent_id=?2 AND e.status='completed' AND c.status='ACTIVE'",params![source,agent],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let pair = if let Some(pair) = pair {
                pair
            } else {
                let pair:Option<(String,String)>=db.query_row("SELECT o.app_id,o.collection FROM app_observations o JOIN apps a ON a.id=o.app_id JOIN app_grants g ON g.app_id=o.app_id AND g.agent_id=o.agent_id AND g.collection=o.collection WHERE o.id=?1 AND o.agent_id=?2 AND a.state='saved'",params![source,agent],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
                let (app, collection) =
                    pair.context("Panel source is unavailable, foreign or no longer granted")?;
                (format!("app:{app}"), collection)
            };
            scope.push(pair);
        }
        scope.sort();
        scope.dedup();
        Ok(scope)
    }
    pub async fn write_panel(&self, run: ExecutionRun<'_>, input: &PanelDraft) -> Result<Value> {
        input.validate()?;
        let _gate = self.gate.write().await;
        ensure!(
            !run.cancel.is_cancelled() && run.store.claim_active(run.claim)?,
            "Panel run is no longer active"
        );
        let mut input: PanelDraft = serde_json::from_value(serde_json::to_value(input)?)?;
        let observed: Vec<String> = {
            let db = self.database()?;
            let mut q = db.prepare(
                "SELECT id FROM executions WHERE agent_id=?1 AND task_id=?2 AND status='completed' UNION SELECT id FROM app_observations WHERE agent_id=?1 AND task_id=?2",
            )?;
            q.query_map(params![run.agent, run.claim.task_id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        input.sources.extend(observed);
        input.sources.sort();
        input.sources.dedup();
        input.validate()?;
        self.panel_source_grants(run.agent, &input.sources)?;
        let automatic = self
            .automatic_panel_update(run.agent, &serde_json::to_value(&input)?)
            .unwrap_or(false);
        let definition = input.definition().to_string();
        let sources = json!(input.sources).to_string();
        let mut db = self.database()?;
        let tx = db.transaction()?;
        let prior: Option<(String, i64, String)> = tx
            .query_row(
                "SELECT agent_id,revision,state FROM panels WHERE id=?1",
                [&input.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let revision = if let Some((agent, revision, state)) = prior {
            ensure!(
                agent == run.agent && state != "removed",
                "Panel is unavailable to this agent"
            );
            let (old_definition, old_sources): (String, String) = tx.query_row(
                "SELECT definition,sources FROM panel_versions WHERE panel_id=?1 AND revision=?2",
                params![input.id, revision],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if old_definition == definition && old_sources == sources {
                if input.publish {
                    ensure!(
                        revision == input.expected_revision,
                        "Panel changed; review its current revision"
                    );
                    tx.execute(
                        "UPDATE panels SET active_revision=?2,updates_enabled=?3 WHERE id=?1",
                        params![input.id, revision, input.updates_enabled],
                    )?;
                    tx.commit()?;
                }
                return Ok(json!({"id":input.id,"revision":revision,"unchanged":true}));
            }
            ensure!(
                revision == input.expected_revision,
                "Panel changed; inspect its current revision before updating"
            );
            revision + 1
        } else {
            ensure!(
                input.expected_revision == 0,
                "New panel must start at revision zero"
            );
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM panels WHERE agent_id=?1 AND state!='removed'",
                [run.agent],
                |r| r.get(0),
            )?;
            ensure!(count < 50, "At most 50 saved panels per agent");
            tx.execute(
                "INSERT INTO panels(id,agent_id,revision) VALUES(?1,?2,0)",
                params![input.id, run.agent],
            )?;
            1
        };
        tx.execute("INSERT INTO panel_versions(panel_id,revision,definition,sources,task_id) VALUES(?1,?2,?3,?4,?5)",params![input.id,revision,definition,sources,run.claim.task_id])?;
        tx.execute(
            "UPDATE panels SET revision=?2 WHERE id=?1",
            params![input.id, revision],
        )?;
        if automatic || input.publish {
            tx.execute(
                "UPDATE panels SET active_revision=?2 WHERE id=?1",
                params![input.id, revision],
            )?;
        }
        if input.publish {
            tx.execute(
                "UPDATE panels SET updates_enabled=?2 WHERE id=?1",
                params![input.id, input.updates_enabled],
            )?;
        }
        tx.commit()?;
        Ok(
            json!({"id":input.id,"revision":revision,"state":if automatic || input.publish {"active"}else{"awaiting_owner_review"}}),
        )
    }
    pub fn panels(&self, agent: Option<&str>) -> Result<Value> {
        let versions: Vec<SavedPanel> = {
            let db = self.database()?;
            let mut q=db.prepare("SELECT p.id,p.agent_id,p.revision,p.active_revision,v.definition,v.sources,v.task_id,v.created_at,p.updates_enabled FROM panels p JOIN panel_versions v ON v.panel_id=p.id AND v.revision=p.revision WHERE p.state!='removed' AND (?1 IS NULL OR p.agent_id=?1) ORDER BY p.rowid DESC LIMIT 200")?;
            q.query_map([agent], |r| {
                Ok(SavedPanel {
                    id: r.get(0)?,
                    agent: r.get(1)?,
                    revision: r.get(2)?,
                    active: r.get(3)?,
                    definition: r.get(4)?,
                    sources: r.get(5)?,
                    task: r.get(6)?,
                    date: r.get(7)?,
                    updates: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?
        };
        let mut panels = Vec::new();
        for SavedPanel {
            id,
            agent,
            revision,
            active,
            definition,
            sources,
            task,
            date,
            updates,
        } in versions
        {
            let sources: Vec<String> = serde_json::from_str(&sources)?;
            let allowed = self.panel_source_grants(&agent, &sources).is_ok();
            let draft: Value = serde_json::from_str(&definition)?;
            let mut active_view = None;
            if let Some(active) = active {
                let (view,source_ids,date):(String,String,String)=self.database()?.query_row("SELECT definition,sources,created_at FROM panel_versions WHERE panel_id=?1 AND revision=?2",params![id,active],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
                if self
                    .panel_source_grants(&agent, &serde_json::from_str::<Vec<String>>(&source_ids)?)
                    .is_ok()
                {
                    active_view = Some(
                        json!({"definition":serde_json::from_str::<Value>(&view)?,"updated_at":date}),
                    );
                }
            }
            panels.push(json!({"id":id,"agent_id":agent,"revision":revision,"active_revision":active,"updates_enabled":updates,"draft":if allowed {Some(draft)}else{None},"active":active_view,"state":if !allowed {"disconnected"} else if active!=Some(revision) {"awaiting_review"}else{"active"},"source_executions":if allowed {sources}else{vec![]},"task_id":task,"updated_at":date}));
        }
        Ok(json!(panels))
    }
    pub async fn activate_panel(
        &self,
        id: &str,
        revision: i64,
        updates_enabled: bool,
    ) -> Result<()> {
        let _gate = self.gate.write().await;
        let (agent,sources):(String,String)=self.database()?.query_row("SELECT p.agent_id,v.sources FROM panels p JOIN panel_versions v ON v.panel_id=p.id AND v.revision=p.revision WHERE p.id=?1 AND p.revision=?2 AND p.state!='removed'",params![id,revision],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.context("Panel changed or is unavailable; refresh its preview")?;
        self.panel_source_grants(&agent, &serde_json::from_str::<Vec<String>>(&sources)?)?;
        self.database()?.execute(
            "UPDATE panels SET active_revision=?2,updates_enabled=?3 WHERE id=?1",
            params![id, revision, updates_enabled],
        )?;
        Ok(())
    }
    pub async fn remove_panel(&self, id: &str, revision: i64) -> Result<()> {
        let _gate = self.gate.write().await;
        let changed=self.database()?.execute("UPDATE panels SET state='removed',updates_enabled=0 WHERE id=?1 AND revision=?2 AND state!='removed'",params![id,revision])?;
        if changed == 0 {
            let removed:bool=self.database()?.query_row("SELECT EXISTS(SELECT 1 FROM panels WHERE id=?1 AND revision=?2 AND state='removed')",params![id,revision],|r|r.get(0))?;
            ensure!(removed, "Panel changed; refresh Home");
        }
        Ok(())
    }
    pub async fn set_panel_updates(&self, id: &str, revision: i64, enabled: bool) -> Result<()> {
        let _gate = self.gate.write().await;
        let changed=self.database()?.execute("UPDATE panels SET updates_enabled=?3 WHERE id=?1 AND revision=?2 AND active_revision=revision AND state='saved'",params![id,revision,enabled])?;
        ensure!(
            changed == 1,
            "Review the current panel revision before changing updates"
        );
        Ok(())
    }
}
