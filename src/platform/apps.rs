use super::{ExecutionRun, Platform};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppDraft {
    pub id: String,
    pub expected_revision: i64,
    pub name: String,
    pub description: String,
    pub html: String,
    pub css: String,
    pub javascript: String,
    pub collections: BTreeMap<String, BTreeMap<String, FieldType>>,
}
#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Number,
    Boolean,
}
pub(super) fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value.as_bytes()[0].is_ascii_lowercase()
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            && !["id", "revision", "constructor", "prototype"].contains(&value),
        "Collection and field names must be descriptive lowercase identifiers, excluding reserved record fields"
    );
    Ok(())
}
impl AppDraft {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&self.id).is_ok_and(|id| id.to_string() == self.id),
            "App ID must be a canonical UUID"
        );
        ensure!(
            self.expected_revision >= 0
                && !self.name.trim().is_empty()
                && self.name.len() <= 100
                && self.description.len() <= 1000,
            "Invalid app name, description or revision"
        );
        ensure!(
            !self.html.trim().is_empty() && serde_json::to_vec(self)?.len() <= 12000,
            "App source exceeds the initial 12000-byte release budget"
        );
        ensure!(
            !self.collections.is_empty() && self.collections.len() <= 12,
            "An app supports 1–12 record collections"
        );
        for (collection, fields) in &self.collections {
            name(collection)?;
            ensure!(
                !fields.is_empty() && fields.len() <= 24,
                "Each collection supports 1–24 named fields"
            );
            for key in fields.keys() {
                name(key)?;
            }
        }
        Ok(())
    }
}
impl Platform {
    pub fn app_source(&self, agent: &str, id: &str, revision: Option<i64>) -> Result<Value> {
        let db = self.database()?;
        let current: i64 = db
            .query_row(
                "SELECT revision FROM apps WHERE id=?1 AND agent_id=?2 AND state='saved'",
                params![id, agent],
                |r| r.get(0),
            )
            .optional()?
            .context("App source is unavailable to this builder")?;
        let revision = revision.unwrap_or(current);
        let definition = Self::app_version_in(&db, id, revision)?;
        Ok(json!({"revision":revision,"definition":definition}))
    }
    pub fn app_agents(&self, id: &str) -> Result<Vec<String>> {
        let db = self.database()?;
        let mut q=db.prepare("SELECT agent_id FROM app_grants WHERE app_id=?1 UNION SELECT agent_id FROM apps WHERE id=?1")?;
        Ok(q.query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub async fn write_app(&self, run: ExecutionRun<'_>, input: &AppDraft) -> Result<Value> {
        input.validate()?;
        let _gate = self.gate.write().await;
        ensure!(
            !run.cancel.is_cancelled() && run.store.claim_active(run.claim)?,
            "Builder run is no longer active"
        );
        let definition = serde_json::to_string(input)?;
        let mut db = self.database()?;
        let tx = db.transaction()?;
        let previous: Option<(String, i64, String)> = tx
            .query_row(
                "SELECT agent_id,revision,state FROM apps WHERE id=?1",
                [&input.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let revision = if let Some((agent, revision, state)) = previous {
            ensure!(
                agent == run.agent && state == "saved",
                "App is unavailable to this builder"
            );
            let prior: String = tx.query_row(
                "SELECT definition FROM app_versions WHERE app_id=?1 AND revision=?2",
                params![input.id, revision],
                |r| r.get(0),
            )?;
            if prior == definition {
                return Ok(json!({"id":input.id,"revision":revision,"unchanged":true}));
            }
            ensure!(
                revision == input.expected_revision,
                "App source changed; inspect the current revision"
            );
            revision + 1
        } else {
            ensure!(
                input.expected_revision == 0,
                "A new app must start at revision zero"
            );
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM apps WHERE agent_id=?1 AND state='saved'",
                [run.agent],
                |r| r.get(0),
            )?;
            ensure!(
                count < 20,
                "At most 20 saved apps per builder in this local increment"
            );
            tx.execute(
                "INSERT INTO apps(id,agent_id,revision) VALUES(?1,?2,0)",
                params![input.id, run.agent],
            )?;
            1
        };
        tx.execute(
            "INSERT INTO app_versions(app_id,revision,definition,task_id) VALUES(?1,?2,?3,?4)",
            params![input.id, revision, definition, run.claim.task_id],
        )?;
        tx.execute(
            "UPDATE apps SET revision=?2 WHERE id=?1",
            params![input.id, revision],
        )?;
        tx.commit()?;
        Ok(json!({"id":input.id,"revision":revision,"state":"preview_required"}))
    }
    pub fn apps(&self, scope_agent: Option<&str>) -> Result<Value> {
        let db = self.database()?;
        let mut q=db.prepare("SELECT a.id,a.agent_id,a.revision,a.active_revision,v.definition,v.created_at FROM apps a JOIN app_versions v ON v.app_id=a.id AND v.revision=a.revision WHERE a.state='saved' AND (?1 IS NULL OR a.agent_id=?1 OR EXISTS(SELECT 1 FROM app_grants g WHERE g.app_id=a.id AND g.agent_id=?1)) ORDER BY a.rowid DESC LIMIT 100")?;
        let rows = q.query_map([scope_agent], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, agent, revision, active, definition, date) = row?;
            let mut draft: AppDraft = serde_json::from_str(&definition)?;
            if let Some(requester) = scope_agent.filter(|scope| *scope != agent) {
                let mut allowed = BTreeMap::new();
                for (key, fields) in draft.collections {
                    let granted:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM app_grants WHERE app_id=?1 AND agent_id=?2 AND collection=?3)",params![id,requester,key],|r|r.get(0))?;
                    if granted {
                        allowed.insert(key, fields);
                    }
                }
                draft.collections = allowed;
            }
            result.push(json!({"id":id,"agent_id":agent,"revision":revision,"active_revision":active,"name":draft.name,"description":draft.description,"collections":draft.collections,"updated_at":date}));
        }
        Ok(json!(result))
    }
    pub(super) fn app_version_in(
        db: &rusqlite::Connection,
        id: &str,
        version: i64,
    ) -> Result<AppDraft> {
        let definition:String=db.query_row("SELECT v.definition FROM app_versions v JOIN apps a ON a.id=v.app_id WHERE v.app_id=?1 AND v.revision=?2 AND a.state='saved'",params![id,version],|r|r.get(0)).optional()?.context("App release is unavailable")?;
        Ok(serde_json::from_str(&definition)?)
    }
    pub async fn app_session(&self, id: &str, revision: i64, preview: bool) -> Result<Value> {
        let _gate = self.gate.write().await;
        let mut db = self.database()?;
        let tx = db.transaction()?;
        let definition = Self::app_version_in(&tx, id, revision)?;
        let active: Option<i64> =
            tx.query_row("SELECT active_revision FROM apps WHERE id=?1", [id], |r| {
                r.get(0)
            })?;
        ensure!(
            preview || active == Some(revision),
            "Open the currently activated app release"
        );
        let now = crate::storage::intake::now_ms();
        tx.execute("DELETE FROM app_sessions WHERE expires_ms<?1", [now])?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM app_sessions", [], |r| r.get(0))?;
        ensure!(
            count < 1000,
            "Too many open app sessions; wait for old sessions to expire"
        );
        let token = uuid::Uuid::new_v4().to_string();
        let document = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO app_sessions VALUES(?1,?2,?3,?4,?5,?6)",
            params![token, id, revision, preview, now + 3600000, document],
        )?;
        tx.commit()?;
        Ok(
            json!({"session":token,"document_id":document,"document_url":format!("/api/apps/{id}/document/{document}"),"revision":revision,"preview":preview,"definition":definition,"expires_ms":now+3600000}),
        )
    }
    pub async fn activate_app(
        &self,
        id: &str,
        revision: i64,
        preview_session: &str,
        expected_active: Option<i64>,
    ) -> Result<()> {
        let _gate = self.gate.write().await;
        let mut db = self.database()?;
        let tx = db.transaction()?;
        let reviewed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM app_sessions WHERE id=?1 AND app_id=?2 AND revision=?3 AND preview=1 AND expires_ms>?4)",params![preview_session,id,revision,crate::storage::intake::now_ms()],|r|r.get(0))?;
        ensure!(
            reviewed,
            "Open and review this exact preview before activation"
        );
        let active: Option<i64> = tx.query_row(
            "SELECT active_revision FROM apps WHERE id=?1 AND state='saved'",
            [id],
            |r| r.get(0),
        )?;
        ensure!(
            active == expected_active,
            "The active app changed; refresh before replacing it"
        );
        let next = Self::app_version_in(&tx, id, revision)?;
        let current: Option<String> = tx
            .query_row(
                "SELECT collections FROM app_schemas WHERE app_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        let mut schema: BTreeMap<String, BTreeMap<String, FieldType>> = current
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();
        for (collection, fields) in next.collections {
            let target = schema.entry(collection).or_default();
            for (key, kind) in fields {
                ensure!(
                    target.get(&key).is_none_or(|old| old == &kind),
                    "Activation cannot change a stored field type; migrate data explicitly first"
                );
                target.insert(key, kind);
            }
        }
        tx.execute("INSERT INTO app_schemas VALUES(?1,?2) ON CONFLICT(app_id) DO UPDATE SET collections=excluded.collections",params![id,serde_json::to_string(&schema)?])?;
        tx.execute(
            "UPDATE apps SET active_revision=?2 WHERE id=?1",
            params![id, revision],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub async fn archive_app(&self, id: &str, revision: i64) -> Result<()> {
        let _gate = self.gate.write().await;
        let agents = self.app_agents(id)?;
        let mut db = self.database()?;
        let tx = db.transaction()?;
        let changed = tx.execute(
            "UPDATE apps SET state='archived' WHERE id=?1 AND revision=?2 AND state='saved'",
            params![id, revision],
        )?;
        ensure!(changed == 1, "App changed or was already archived");
        for agent in agents {
            Self::bump_authority_in(&tx, &agent)?;
        }
        tx.commit()?;
        Ok(())
    }
}
