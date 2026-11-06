//! Agents acting in built apps (Apps AP10). The manifest offers actions; the builder or
//! an admin allows each one for agents (blocked until then). An agent runs an allowed
//! action for the person it works for, through the same checks as the person's own
//! requests, so it can never do more in the app than they could. Each run is kept by a
//! one-time key, recorded with the agent's name, and audited.
use super::data::{DataRequest, answer};
use super::*;
use crate::identity::built_apps::manifest::Manifest;
use crate::identity::chat::model::Job;
use crate::identity::library::model::Conflict;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Schema 81: which app actions agents may run, and the runs.
pub(crate) struct BuiltAppActions;
impl MigrationName for BuiltAppActions {
    fn name(&self) -> &str {
        "m20261105_000081_built_app_actions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for BuiltAppActions {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("built_app_actions.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Built app actions require a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentRule {
    pub action: String,
    /// allow or block.
    pub mode: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunAction {
    pub app: String,
    pub action: String,
    pub record: Option<String>,
    pub data: Option<Value>,
    pub revision: Option<i64>,
}

/// The manifest people use now: the published version's, or the draft's before that.
async fn current(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
    published: i32,
) -> Result<Option<Manifest>> {
    let text = query(tx, "SELECT content FROM relay_built_app_files WHERE workspace=$1 AND app=$2 AND version=$3 AND path='cuckoo-app.json'", vec![w.into(), id.into(), published.into()]).await?;
    Ok(match text {
        Some(row) => Manifest::parse(&row.try_get::<String>("", "content")?).ok(),
        None => None,
    })
}
async fn modes(tx: &DatabaseTransaction, w: &str, id: &str) -> Result<Vec<(String, String)>> {
    rows(
        tx,
        "SELECT action,mode FROM relay_built_app_agent_rules WHERE workspace=$1 AND app=$2",
        vec![w.into(), id.into()],
    )
    .await?
    .into_iter()
    .map(|r| Ok((r.try_get("", "action")?, r.try_get("", "mode")?)))
    .collect()
}
async fn person(tx: &DatabaseTransaction, id: &str) -> Result<User> {
    let row = query(
        tx,
        "SELECT id,email,display_name FROM relay_users WHERE id=$1",
        vec![id.into()],
    )
    .await?
    .context("Person")?;
    Ok(User {
        id: row.try_get("", "id")?,
        email: row.try_get("", "email")?,
        display_name: row.try_get("", "display_name")?,
    })
}
fn refused(message: impl Into<String>) -> Value {
    json!({"state":"refused","message":message.into()})
}

impl PgStore {
    /// What agents may do in the app, for anyone who can see it.
    pub(crate) async fn built_app_agent_rules(
        &self,
        user: &User,
        w: &str,
        id: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let (found, _) = super::app(&tx, user, w, id).await?;
        let version = found["published"].as_i64().unwrap_or(0) as i32;
        let manifest = current(&tx, w, id, version).await?;
        let set = modes(&tx, w, id).await?;
        tx.commit().await?;
        let actions: Vec<Value> = manifest.iter().flat_map(|m| &m.actions).map(|a| {
            let mode = set.iter().find(|(action, _)| *action == a.id).map_or("block", |(_, m)| m.as_str());
            json!({"id":a.id,"title":a.title,"does":a.does,"collection":a.collection,"fields":a.fields,"mode":mode})
        }).collect();
        Ok(json!({"actions":actions,"can_change":found["manages"],"from_version":version}))
    }

    /// The builder or an admin allows or blocks one action for agents.
    pub(crate) async fn built_app_agent_rule(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &AgentRule,
    ) -> Result<Value> {
        ensure!(
            ["allow", "ask", "block"].contains(&input.mode.as_str()),
            "Allow it, ask first or block it"
        );
        let tx = self.workspace_tx(w).await?;
        let (found, _) = super::app(&tx, user, w, id).await?;
        ensure!(
            found["manages"] == true,
            "Its builder or an admin decides what agents may do"
        );
        let version = found["published"].as_i64().unwrap_or(0) as i32;
        let manifest = current(&tx, w, id, version)
            .await?
            .context("Fix the app's manifest first")?;
        let action = manifest
            .actions
            .iter()
            .find(|a| a.id == input.action)
            .with_context(|| format!("The app has no action \"{}\"", input.action))?;
        execute(&tx, "INSERT INTO relay_built_app_agent_rules(workspace,app,action,mode,set_by,set_at) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(workspace,app,action) DO UPDATE SET mode=EXCLUDED.mode,set_by=EXCLUDED.set_by,set_at=EXCLUDED.set_at", vec![w.into(), id.into(), input.action.clone().into(), input.mode.clone().into(), user.id.clone().into(), now().into()]).await?;
        let name = found["name"].as_str().unwrap_or("");
        let said = if input.mode == "allow" {
            format!("Agents may \"{}\" in \"{name}\"", action.title)
        } else if input.mode == "ask" {
            format!(
                "Agents may \"{}\" in \"{name}\" after asking the person",
                action.title
            )
        } else {
            format!("Agents may no longer \"{}\" in \"{name}\"", action.title)
        };
        audit_with_summary(&tx, &user.id, Some(w), "built_app_agent_rule", id, &said).await?;
        tx.commit().await?;
        Ok(json!({"ok":true}))
    }

    /// Whether this person has any app action agents may run, for the instructions.
    pub(crate) async fn built_app_actions_offered(&self, job: &Job) -> Result<bool> {
        let present: bool = query(
            &self.db,
            "SELECT to_regclass('relay_built_app_agent_rules') IS NOT NULL AS present",
            vec![],
        )
        .await?
        .context("Schema check")?
        .try_get("", "present")?;
        Ok(present && query(&self.db, "SELECT 1 AS found FROM relay_built_app_agent_rules r JOIN relay_built_apps a ON a.workspace=r.workspace AND a.id=r.app WHERE r.workspace=$1 AND r.mode IN ('allow','ask') AND a.published IS NOT NULL AND NOT a.removed LIMIT 1", vec![job.workspace.clone().into()]).await?.is_some())
    }

    /// Agent tool `app_actions`: what the agent may do, for this person, in each app
    /// they can open.
    pub(crate) async fn agent_app_actions(&self, job: &Job) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let requester = person(&tx, &job.requester).await?;
        let ids: Vec<String> = rows(&tx, "SELECT DISTINCT r.app FROM relay_built_app_agent_rules r JOIN relay_built_apps a ON a.workspace=r.workspace AND a.id=r.app WHERE r.workspace=$1 AND r.mode IN ('allow','ask') AND a.published IS NOT NULL AND NOT a.removed ORDER BY r.app", vec![job.workspace.clone().into()]).await?.into_iter().map(|r| r.try_get("", "app")).collect::<std::result::Result<_, _>>()?;
        let mut apps = vec![];
        for id in ids {
            let Ok((found, _)) = super::app(&tx, &requester, &job.workspace, &id).await else {
                continue;
            };
            if found["opens"] != true {
                continue;
            }
            let Some(manifest) = current(
                &tx,
                &job.workspace,
                &id,
                found["published"].as_i64().unwrap_or(0) as i32,
            )
            .await?
            else {
                continue;
            };
            let set = modes(&tx, &job.workspace, &id).await?;
            let actions: Vec<Value> = manifest.actions.iter().filter(|a| set.iter().any(|(action, mode)| *action == a.id && mode != "block")).map(|a| {
                let types = &manifest.collections[&a.collection].fields;
                json!({"id":a.id,"title":a.title,"does":a.does,"asks_first":set.iter().any(|(action, mode)| *action == a.id && mode == "ask"),"fields":a.fields.iter().map(|f| json!({"name":f,"type":types[f]})).collect::<Vec<_>>()})
            }).collect();
            if !actions.is_empty() {
                apps.push(json!({"app":id,"name":found["name"],"actions":actions}));
            }
        }
        tx.commit().await?;
        Ok(json!({"apps":apps}))
    }

    /// Agent tool `run_app_action`: runs an allowed action as the person, once per key.
    pub(crate) async fn agent_run_app_action(&self, job: &Job, input: &RunAction) -> Result<Value> {
        let key = crate::identity::provider::digest(&format!(
            "{}\n{}",
            job.reply,
            serde_json::to_string(input)?
        ));
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        if let Some(row) = query(
            &tx,
            "SELECT result FROM relay_built_app_action_runs WHERE workspace=$1 AND key=$2",
            vec![job.workspace.clone().into(), key.clone().into()],
        )
        .await?
        {
            tx.commit().await?;
            let mut result: Value = row.try_get("", "result")?;
            result["repeated"] = true.into();
            return Ok(result);
        }
        let requester = person(&tx, &job.requester).await?;
        let Ok((found, _)) = super::app(&tx, &requester, &job.workspace, &input.app).await else {
            tx.commit().await?;
            return Ok(refused("This person can't open that app."));
        };
        let Some(manifest) = current(
            &tx,
            &job.workspace,
            &input.app,
            found["published"].as_i64().unwrap_or(0) as i32,
        )
        .await?
        .filter(|_| found["opens"] == true) else {
            tx.commit().await?;
            return Ok(refused("This person can't open that app."));
        };
        let mode = modes(&tx, &job.workspace, &input.app)
            .await?
            .into_iter()
            .find(|(action, _)| *action == input.action)
            .map_or_else(|| "block".to_owned(), |(_, mode)| mode);
        let allowed = mode != "block";
        let Some(action) = manifest
            .actions
            .iter()
            .find(|a| a.id == input.action)
            .filter(|_| allowed)
        else {
            tx.commit().await?;
            return Ok(refused(
                "Agents may not do that in this app. Tell the person to do it in the app themselves.",
            ));
        };
        if let Some(fields) = input.data.as_ref().and_then(Value::as_object) {
            if let Some(extra) = fields.keys().find(|f| !action.fields.contains(f)) {
                tx.commit().await?;
                return Ok(refused(format!(
                    "\"{}\" sets only {}; not \"{extra}\".",
                    action.title,
                    action.fields.join(", ")
                )));
            }
        }
        if mode == "ask" {
            let agent: String = query(
                &tx,
                "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
                vec![job.workspace.clone().into(), job.agent.clone().into()],
            )
            .await?
            .context("Agent")?
            .try_get("", "name")?;
            let (key, specification) = super::asking::question(
                &agent,
                &action.title,
                found["name"].as_str().unwrap_or(""),
                &serde_json::to_value(input)?,
            )?;
            match super::asking::answered(&tx, job, &key, &specification).await? {
                None => {
                    tx.commit().await?;
                    return Ok(
                        json!({"state":"needs_confirmation","message":"This action asks the person first. Call ask_question with exactly this key and specification; once they answer, call run_app_action again with the same fields.","question":{"key":key,"specification":specification}}),
                    );
                }
                Some(false) => {
                    tx.commit().await?;
                    return Ok(refused("The person didn't agree, so it wasn't done."));
                }
                Some(true) => {}
            }
        }
        let op = match (action.does.as_str(), input.record.is_some()) {
            ("read", false) => "list",
            ("read", true) => "get",
            (does, _) => does,
        };
        let request = DataRequest {
            preview: false,
            op: op.into(),
            collection: action.collection.clone(),
            record: input.record.clone(),
            data: input.data.clone(),
            revision: input.revision,
            mine: false,
        };
        let result = match answer(
            &tx,
            &requester,
            &job.workspace,
            &input.app,
            &request,
            Some(&job.agent),
        )
        .await
        {
            Ok(result) => json!({"state":"done","result":result}),
            Err(error) if error.is::<Conflict>() => {
                tx.commit().await?;
                return Ok(
                    json!({"state":"changed","message":"The record changed since you read it. Read it again."}),
                );
            }
            Err(error) => {
                tx.commit().await?;
                return Ok(refused(error.to_string()));
            }
        };
        if action.does != "read" {
            let agent: String = query(
                &tx,
                "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
                vec![job.workspace.clone().into(), job.agent.clone().into()],
            )
            .await?
            .context("Agent")?
            .try_get("", "name")?;
            let who = requester
                .display_name
                .clone()
                .unwrap_or_else(|| requester.email.clone());
            audit_with_summary(
                &tx,
                &job.requester,
                Some(&job.workspace),
                "built_app_agent_action",
                &input.app,
                &format!(
                    "{agent} did \"{}\" in \"{}\" for {who}",
                    action.title,
                    found["name"].as_str().unwrap_or("")
                ),
            )
            .await?;
            execute(&tx, "INSERT INTO relay_built_app_action_runs(workspace,key,app,action,agent,requester,result,at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)", vec![job.workspace.clone().into(), key.into(), input.app.clone().into(), input.action.clone().into(), job.agent.clone().into(), job.requester.clone().into(), result.clone().into(), now().into()]).await?;
        }
        tx.commit().await?;
        Ok(result)
    }
}

#[cfg(test)]
#[path = "built_app_actions_tests.rs"]
mod tests;
