//! Agents that build apps (Apps BA3). An app's builder asks an agent to work on its
//! draft; that agent then reads and writes the draft's files, but only in requests from
//! the builder. It can't publish: the builder previews and publishes, as with their own
//! changes. Every file the agent writes is audited.
use super::code::{files, write_draft};
use super::*;
use crate::identity::built_apps::files as built_apps;
use crate::identity::chat::model::Job;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;
use serde_json::json;

/// Schema 80: agents asked to build apps.
pub(crate) struct BuiltAppAgents;
impl MigrationName for BuiltAppAgents {
    fn name(&self) -> &str {
        "m20261105_000080_built_app_agents"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for BuiltAppAgents {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("built_app_agents.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Built app agents require a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Helper {
    pub agent: String,
    /// True asks the agent to help; false stops it.
    pub helps: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppRef {
    pub app: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadFile {
    pub app: String,
    pub path: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteFile {
    pub app: String,
    pub path: String,
    pub content: String,
    pub revision: i64,
}

/// The app if this agent was asked to build it and the request is from its builder.
async fn helping(tx: &DatabaseTransaction, job: &Job, app: &str) -> Result<Option<Value>> {
    Ok(json_rows(tx, "SELECT jsonb_build_object('id',a.id,'name',a.name,'description',a.description,'draft_revision',a.draft_revision,'version',a.published) AS data FROM relay_built_apps a JOIN relay_built_app_helpers h ON h.workspace=a.workspace AND h.app=a.id WHERE a.workspace=$1 AND a.id=$2 AND h.agent=$3 AND a.builder=$4 AND NOT a.removed", vec![job.workspace.clone().into(), app.into(), job.agent.clone().into(), job.requester.clone().into()]).await?.into_iter().next())
}
fn not_yours() -> Value {
    json!({"state":"not_allowed","message":"You weren't asked to build this app for this person. Only its builder can ask you, from the app on the Apps page."})
}

impl PgStore {
    /// The builder asks an agent to work on the app's draft, or stops it.
    pub(crate) async fn built_app_helper(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Helper,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let (found, _) = super::app(&tx, user, w, id).await?;
        ensure!(
            found["builder"] == user.id.as_str(),
            "The person building the app asks an agent to help"
        );
        let agent: String = query(
            &tx,
            "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
            vec![w.into(), input.agent.clone().into()],
        )
        .await?
        .context("Choose an agent in this workspace")?
        .try_get("", "name")?;
        let name = found["name"].as_str().unwrap_or("");
        if input.helps {
            execute(&tx, "INSERT INTO relay_built_app_helpers(workspace,app,agent,given_by,given_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING", vec![w.into(), id.into(), input.agent.clone().into(), user.id.clone().into(), now().into()]).await?;
        } else {
            execute(
                &tx,
                "DELETE FROM relay_built_app_helpers WHERE workspace=$1 AND app=$2 AND agent=$3",
                vec![w.into(), id.into(), input.agent.clone().into()],
            )
            .await?;
        }
        let said = if input.helps {
            format!("Asked {agent} to build \"{name}\"")
        } else {
            format!("Stopped {agent} building \"{name}\"")
        };
        audit_with_summary(&tx, &user.id, Some(w), "built_app_helper", id, &said).await?;
        tx.commit().await?;
        Ok(json!({"ok":true}))
    }

    /// Whether this agent builds any app for this person, for its instructions.
    pub(crate) async fn built_app_agent_helps(&self, job: &Job) -> Result<bool> {
        let present: bool = query(
            &self.db,
            "SELECT to_regclass('relay_built_app_helpers') IS NOT NULL AS present",
            vec![],
        )
        .await?
        .context("Schema check")?
        .try_get("", "present")?;
        Ok(present && query(&self.db, "SELECT 1 AS found FROM relay_built_app_helpers h JOIN relay_built_apps a ON a.workspace=h.workspace AND a.id=h.app WHERE h.workspace=$1 AND h.agent=$2 AND a.builder=$3 AND NOT a.removed LIMIT 1", vec![job.workspace.clone().into(), job.agent.clone().into(), job.requester.clone().into()]).await?.is_some())
    }

    /// Agent tool `my_apps`: the apps this person asked the agent to build.
    pub(crate) async fn agent_my_apps(&self, job: &Job) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let apps = json_rows(&tx, "SELECT jsonb_build_object('id',a.id,'name',a.name,'description',a.description,'draft_revision',a.draft_revision,'published_version',a.published) AS data FROM relay_built_apps a JOIN relay_built_app_helpers h ON h.workspace=a.workspace AND h.app=a.id WHERE a.workspace=$1 AND h.agent=$2 AND a.builder=$3 AND NOT a.removed ORDER BY a.name,a.id", vec![job.workspace.clone().into(), job.agent.clone().into(), job.requester.clone().into()]).await?;
        tx.commit().await?;
        Ok(json!({"apps":apps}))
    }

    /// Agent tool `app_files`: the draft's files, its revision and what the checks find.
    pub(crate) async fn agent_app_files(&self, job: &Job, input: &AppRef) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let Some(app) = helping(&tx, job, &input.app).await? else {
            tx.commit().await?;
            return Ok(not_yours());
        };
        let draft = files(&tx, &job.workspace, &input.app, 0).await?;
        let errors = super::code::preview_errors(
            &tx,
            &job.workspace,
            &input.app,
            app["draft_revision"].as_i64().unwrap_or(0),
        )
        .await?;
        tx.commit().await?;
        let list: Vec<Value> = draft
            .iter()
            .map(|(path, content)| json!({"path":path,"size":content.len()}))
            .collect();
        Ok(
            json!({"app":app,"files":list,"checks":built_apps::checks(&draft),"preview_errors":errors}),
        )
    }

    /// Agent tool `read_app_file`: one draft file.
    pub(crate) async fn agent_read_app_file(&self, job: &Job, input: &ReadFile) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let Some(app) = helping(&tx, job, &input.app).await? else {
            tx.commit().await?;
            return Ok(not_yours());
        };
        let content = files(&tx, &job.workspace, &input.app, 0)
            .await?
            .into_iter()
            .find(|(p, _)| *p == input.path)
            .map(|(_, c)| c);
        tx.commit().await?;
        Ok(match content {
            Some(content) => {
                json!({"path":input.path,"content":content,"draft_revision":app["draft_revision"]})
            }
            None => {
                json!({"state":"missing","message":format!("The draft has no file {}", input.path)})
            }
        })
    }

    /// Agent tool `write_app_file`: writes one draft file from the revision it read,
    /// and says what the checks find now.
    pub(crate) async fn agent_write_app_file(&self, job: &Job, input: &WriteFile) -> Result<Value> {
        built_apps::path(&input.path)?;
        ensure!(
            input.content.len() <= built_apps::MAX_FILE,
            "A file is at most 256 KiB"
        );
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let Some(app) = helping(&tx, job, &input.app).await? else {
            tx.commit().await?;
            return Ok(not_yours());
        };
        if app["draft_revision"].as_i64() != Some(input.revision) {
            tx.commit().await?;
            return Ok(
                json!({"state":"changed","message":"The draft changed since you read it. Call app_files and read the file again.","draft_revision":app["draft_revision"]}),
            );
        }
        let revision = write_draft(
            &tx,
            &job.workspace,
            &input.app,
            &input.path,
            Some(&input.content),
        )
        .await?;
        let agent: String = query(
            &tx,
            "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
            vec![job.workspace.clone().into(), job.agent.clone().into()],
        )
        .await?
        .context("Agent")?
        .try_get("", "name")?;
        audit_with_summary(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "built_app_agent_write",
            &input.app,
            &format!(
                "{agent} changed {} in \"{}\"",
                input.path,
                app["name"].as_str().unwrap_or("")
            ),
        )
        .await?;
        let draft = files(&tx, &job.workspace, &input.app, 0).await?;
        tx.commit().await?;
        Ok(
            json!({"state":"saved","draft_revision":revision,"checks":built_apps::checks(&draft),"next":"When the checks pass, tell the person to preview the draft and publish it from Apps; you can't publish."}),
        )
    }
}

#[cfg(test)]
#[path = "built_app_agents_tests.rs"]
mod tests;
