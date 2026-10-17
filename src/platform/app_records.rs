use super::{
    ExecutionRun, Platform,
    apps::{AppDraft, FieldType},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

#[derive(Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecordCommand {
    List {
        collection: String,
        after: Option<String>,
    },
    Save {
        collection: String,
        id: Option<String>,
        expected_revision: i64,
        values: Map<String, Value>,
        request_id: String,
    },
    Delete {
        collection: String,
        id: String,
        expected_revision: i64,
        request_id: String,
    },
}
impl RecordCommand {
    fn collection(&self) -> &str {
        match self {
            Self::List { collection, .. }
            | Self::Save { collection, .. }
            | Self::Delete { collection, .. } => collection,
        }
    }
    fn writing(&self) -> bool {
        !matches!(self, Self::List { .. })
    }
    fn receipt(&self) -> Option<&str> {
        match self {
            Self::Save { request_id, .. } | Self::Delete { request_id, .. } => Some(request_id),
            _ => None,
        }
    }
}
struct RecordScope<'a> {
    app: &'a str,
    dataset: &'a str,
    definition: &'a AppDraft,
}
impl Platform {
    pub async fn session_records(
        &self,
        id: &str,
        session: &str,
        command: &RecordCommand,
    ) -> Result<Value> {
        let _gate = self.gate.write().await;
        let mut db = self.database()?;
        let tx = db.transaction()?;
        let (revision,preview):(i64,bool)=tx.query_row("SELECT s.revision,s.preview FROM app_sessions s JOIN apps a ON a.id=s.app_id WHERE s.id=?1 AND s.app_id=?2 AND s.expires_ms>?3 AND a.state='saved' AND (s.preview=1 OR s.revision=a.active_revision)",params![session,id,crate::storage::intake::now_ms()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.context("App session expired, archived or superseded; reopen the app")?;
        let definition = Self::app_version_in(&tx, id, revision)?;
        let dataset = if preview {
            format!("preview:{revision}")
        } else {
            "live".into()
        };
        let value = Self::records_in(
            &tx,
            RecordScope {
                app: id,
                dataset: &dataset,
                definition: &definition,
            },
            command,
        )?;
        tx.commit()?;
        Ok(value)
    }
    fn records_in(
        tx: &rusqlite::Transaction<'_>,
        scope: RecordScope<'_>,
        command: &RecordCommand,
    ) -> Result<Value> {
        let collection = command.collection();
        let fields = scope
            .definition
            .collections
            .get(collection)
            .context("Collection is not declared by this app release")?;
        let app = scope.app;
        let dataset = scope.dataset;
        if let RecordCommand::List { after, .. } = command {
            if let Some(after) = after {
                ensure!(
                    uuid::Uuid::parse_str(after).is_ok(),
                    "Invalid record cursor"
                );
            }
            let mut q=tx.prepare("SELECT id,revision,data FROM app_records WHERE app_id=?1 AND dataset=?2 AND collection=?3 AND deleted=0 AND id>?4 ORDER BY id LIMIT 101")?;
            let mut rows=q.query_map(params![app,dataset,collection,after.as_deref().unwrap_or("")],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?)))?.map(|r|{let(id,revision,values)=r?;Ok(json!({"id":id,"revision":revision,"values":serde_json::from_str::<Value>(&values)?}))}).collect::<Result<Vec<_>>>()?;
            let more = rows.len() > 100;
            if more {
                rows.pop();
            }
            let cursor = if more {
                rows.last().map(|v| v["id"].clone())
            } else {
                None
            };
            return Ok(json!({"records":rows,"next_cursor":cursor}));
        }
        let receipt = command
            .receipt()
            .context("Record writes require a request ID")?;
        ensure!(
            uuid::Uuid::parse_str(receipt).is_ok(),
            "Record request ID must be a UUID"
        );
        let encoded = serde_json::to_string(command)?;
        ensure!(
            encoded.len() <= 8000,
            "Record mutation exceeds the request limit"
        );
        if let Some((previous,result))=tx.query_row("SELECT command,result FROM app_record_receipts WHERE app_id=?1 AND dataset=?2 AND request_id=?3",params![app,dataset,receipt],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()? {
            ensure!(previous==encoded,"Record request ID was used for different content");return Ok(serde_json::from_str(&result)?)
        }
        let history: i64 = tx.query_row(
            "SELECT COUNT(*) FROM app_record_history WHERE app_id=?1",
            [app],
            |r| r.get(0),
        )?;
        ensure!(
            history < 10000,
            "This local app reached its 10000-mutation history budget"
        );
        let (id, expected, updates, deleted) = match command {
            RecordCommand::Save {
                id,
                expected_revision,
                values,
                ..
            } => (
                id.clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                *expected_revision,
                Some(values),
                false,
            ),
            RecordCommand::Delete {
                id,
                expected_revision,
                ..
            } => (id.clone(), *expected_revision, None, true),
            RecordCommand::List { .. } => unreachable!(),
        };
        ensure!(
            uuid::Uuid::parse_str(&id).is_ok() && expected >= 0,
            "Invalid record identity or revision"
        );
        let previous:Option<(i64,String,bool)>=tx.query_row("SELECT revision,data,deleted FROM app_records WHERE app_id=?1 AND dataset=?2 AND collection=?3 AND id=?4",params![app,dataset,collection,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let mut values = if let Some((revision, values, was_deleted)) = &previous {
            ensure!(
                *revision == expected && !was_deleted,
                "Record changed or was deleted; refresh before saving"
            );
            serde_json::from_str::<Map<String, Value>>(values)?
        } else {
            ensure!(expected == 0 && !deleted, "Record does not exist");
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM app_records WHERE app_id=?1 AND dataset=?2 AND collection=?3",
                params![app, dataset, collection],
                |r| r.get(0),
            )?;
            ensure!(
                count < 1000,
                "Collection reached the local 1000-record budget"
            );
            Map::new()
        };
        if let Some(updates) = updates {
            for (field, value) in updates {
                let kind = fields
                    .get(field)
                    .context("Record field is not declared by this app release")?;
                ensure!(
                    value.is_null()
                        || match kind {
                            FieldType::String => value.is_string(),
                            FieldType::Number => value.is_number(),
                            FieldType::Boolean => value.is_boolean(),
                        },
                    "Record value has the wrong field type"
                );
                values.insert(field.clone(), value.clone());
            }
        }
        let values = serde_json::to_string(&values)?;
        ensure!(
            values.len() <= 4000,
            "Record exceeds the 4000-byte storage budget"
        );
        let revision = previous.map_or(1, |(revision, _, _)| revision + 1);
        tx.execute("INSERT INTO app_records(app_id,dataset,collection,id,revision,data,deleted) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(app_id,dataset,collection,id) DO UPDATE SET revision=excluded.revision,data=excluded.data,deleted=excluded.deleted",params![app,dataset,collection,id,revision,values,deleted])?;
        tx.execute("INSERT INTO app_record_history(app_id,dataset,collection,id,revision,data,deleted) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![app,dataset,collection,id,revision,values,deleted])?;
        let result = json!({"id":id,"revision":revision,"deleted":deleted});
        tx.execute(
            "INSERT INTO app_record_receipts VALUES(?1,?2,?3,?4,?5)",
            params![app, dataset, receipt, encoded, result.to_string()],
        )?;
        Ok(result)
    }
    pub fn app_grants(&self, id: &str) -> Result<Value> {
        let db = self.database()?;
        let mut q =
            db.prepare("SELECT agent_id,collection,can_write FROM app_grants WHERE app_id=?1")?;
        Ok(json!(q.query_map([id],|r|Ok(json!({"agent_id":r.get::<_,String>(0)?,"collection":r.get::<_,String>(1)?,"can_write":r.get::<_,bool>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?))
    }
    pub async fn set_app_grant(
        &self,
        id: &str,
        agent: &str,
        collection: &str,
        enabled: bool,
        can_write: bool,
    ) -> Result<()> {
        let _gate = self.gate.write().await;
        let mut db = self.database()?;
        let tx = db.transaction()?;
        if enabled {
            let active: Option<i64> = tx
                .query_row(
                    "SELECT active_revision FROM apps WHERE id=?1 AND state='saved'",
                    [id],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            let definition = Self::app_version_in(
                &tx,
                id,
                active.context("Activate this app before assigning record access")?,
            )?;
            ensure!(
                definition.collections.contains_key(collection),
                "Unknown app collection"
            );
            tx.execute("INSERT INTO app_grants VALUES(?1,?2,?3,?4) ON CONFLICT(app_id,agent_id,collection) DO UPDATE SET can_write=excluded.can_write",params![id,agent,collection,can_write])?;
        } else {
            tx.execute(
                "DELETE FROM app_grants WHERE app_id=?1 AND agent_id=?2 AND collection=?3",
                params![id, agent, collection],
            )?;
            Self::bump_authority_in(&tx, agent)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub async fn agent_app_records(
        &self,
        run: ExecutionRun<'_>,
        id: &str,
        command: &RecordCommand,
    ) -> Result<Value> {
        let _gate = self.gate.write().await;
        let mut db = self.database()?;
        let tx = db.transaction()?;
        ensure!(
            !run.cancel.is_cancelled() && run.store.claim_active(run.claim)?,
            "App record run is no longer active"
        );
        let grant:Option<bool>=tx.query_row("SELECT can_write FROM app_grants WHERE app_id=?1 AND agent_id=?2 AND collection=?3",params![id,run.agent,command.collection()],|r|r.get(0)).optional()?;
        ensure!(
            grant.is_some() && (!command.writing() || grant == Some(true)),
            "This agent has no current app record grant for the requested operation"
        );
        let active: Option<i64> = tx.query_row(
            "SELECT active_revision FROM apps WHERE id=?1 AND state='saved'",
            [id],
            |r| r.get(0),
        )?;
        let definition = Self::app_version_in(&tx, id, active.context("App is not active")?)?;
        let value = Self::records_in(
            &tx,
            RecordScope {
                app: id,
                dataset: "live",
                definition: &definition,
            },
            command,
        )?;
        let observation = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO app_observations(id,agent_id,task_id,app_id,collection) VALUES(?1,?2,?3,?4,?5)",params![observation,run.agent,run.claim.task_id,id,command.collection()])?;
        tx.commit()?;
        Ok(json!({"observation_id":observation,"data":value}))
    }
}
