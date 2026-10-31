//! How agents use skills (D234 option B): the names and descriptions go into the
//! instructions Cuckoo sends, and `load_skill` returns a file from the version
//! current when the request started. Only the agent's assigned skills exist for
//! it, and every load is recorded for Activity.
use super::*;
use crate::identity::chat::model::Job;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LoadSkill {
    pub name: String,
    /// A supporting file such as `references/prices.csv`; SKILL.md when absent.
    pub file: Option<String>,
}

/// Whether this schema has skills, so current code also runs on older ones.
async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_skills') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .map(|r| r.try_get::<bool>("", "ready"))
    .transpose()?
    .unwrap_or(false))
}

/// "name: description" for each skill the agent may use, for its instructions.
pub(crate) async fn listing(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
) -> Result<Vec<(String, String)>> {
    if !ready(db).await? {
        return Ok(vec![]);
    }
    let rows = rows(db, "SELECT s.name,s.description FROM relay_agent_skills x JOIN relay_skills s ON s.workspace=x.workspace AND s.id=x.skill
        WHERE x.workspace=$1 AND x.agent=$2 ORDER BY s.name LIMIT $3", vec![w.into(), agent.into(), super::AGENT_SKILLS.into()]).await?;
    rows.into_iter()
        .map(|r| Ok((r.try_get("", "name")?, r.try_get("", "description")?)))
        .collect()
}

/// "Used skill: quote-builder (version 3)" steps for one request's details.
pub(crate) async fn steps(db: &impl ConnectionTrait, w: &str, reply: &str) -> Result<Vec<Value>> {
    if !ready(db).await? {
        return Ok(vec![]);
    }
    json_rows(db, "SELECT jsonb_build_object('id','skill-'||l.at_ms||'-'||s.name||COALESCE('/'||l.path,''),'title',
        CASE WHEN l.path IS NULL THEN 'Used skill: '||s.name||' (version '||l.version||')' ELSE 'Read '||l.path||' from '||s.name END,
        'effect','skill','state','completed','automatic',true,'decision',NULL,'decided_by',NULL,'account',NULL,'arguments','{}'::jsonb,'at',l.at_ms/1000) AS data
        FROM relay_skill_loads l JOIN relay_skills s ON s.workspace=l.workspace AND s.id=l.skill
        WHERE l.workspace=$1 AND l.job=$2 ORDER BY l.at_ms LIMIT 50", vec![w.into(), reply.into()]).await
}

impl PgStore {
    /// The agent's skills for its instructions.
    pub async fn agent_skill_listing(&self, w: &str, agent: &str) -> Result<Vec<(String, String)>> {
        listing(&self.db, w, agent).await
    }

    pub(crate) async fn load_skill(&self, job: &Job, input: &LoadSkill) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        let found = query(&tx, "SELECT s.id,s.name,v.version,v.files,v.has_scripts FROM relay_agent_skills x
            JOIN relay_skills s ON s.workspace=x.workspace AND s.id=x.skill
            JOIN LATERAL (SELECT version,files,has_scripts FROM relay_skill_versions v WHERE v.workspace=s.workspace AND v.skill=s.id
              AND v.created_ms<=COALESCE((SELECT created_ms FROM relay_chat_jobs j WHERE j.reply=$4),v.created_ms)
              ORDER BY version DESC LIMIT 1) v ON TRUE
            WHERE x.workspace=$1 AND x.agent=$2 AND s.name=$3",
            vec![job.workspace.clone().into(), job.agent.clone().into(), input.name.clone().into(), job.reply.clone().into()]).await?;
        let Some(row) = found else {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_found","message":format!("No skill named {} is available to you. Use only the skills listed in your instructions.", input.name.chars().take(64).collect::<String>())}),
            );
        };
        let (id, version): (String, i32) = (row.try_get("", "id")?, row.try_get("", "version")?);
        let files: Vec<super::files::SkillFile> =
            serde_json::from_value(row.try_get("", "files")?)?;
        let path = input.file.clone().unwrap_or_else(|| "SKILL.md".into());
        let Some(file) = files.iter().find(|f| f.path == path) else {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_found","message":"That file isn't in this skill.","files":files.iter().map(|f| &f.path).collect::<Vec<_>>()}),
            );
        };
        execute(&tx, "INSERT INTO relay_skill_loads(workspace,job,skill,version,path) VALUES($1,$2,$3,$4,$5)",
            vec![job.workspace.clone().into(), job.reply.clone().into(), id.into(), version.into(), input.file.clone().into()]).await?;
        tx.commit().await?;
        let has_scripts: bool = row.try_get("", "has_scripts")?;
        Ok(
            json!({"state":"available","name":input.name,"version":version,"file":path,"content":file.text,
            "files":files.iter().map(|f| &f.path).collect::<Vec<_>>(),
            "note": if has_scripts { "Scripts in this skill can't run here. Follow the written steps instead." } else { "" }}),
        )
    }
}
