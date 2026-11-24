//! The three-day skill review: which agents are due, what each one's routine
//! work looks like (job types done at least five times on three days in 30
//! days), and saving a review with what it proposes.
use super::super::skills::files::SkillFile;
use super::*;
use serde_json::json;

/// At most one review this often per agent, and only after this much new work.
pub(crate) const EVERY_MS: i64 = 72 * 3_600_000;
pub(crate) const NEW_REQUESTS: i64 = 10;
/// A job type is routine when done this often, on this many days.
pub(crate) const ROUTINE_RUNS: i64 = 5;
pub(crate) const ROUTINE_DAYS: i64 = 3;
/// A rejected proposal's job type waits this long before it is proposed again.
pub(crate) const REJECTED_WAIT_MS: i64 = 30 * 86_400_000;
/// A skill nobody loaded for this long, created this long ago, may be retired.
pub(crate) const UNUSED_MS: i64 = 60 * 86_400_000;
/// Conversations two or more people read: their text may shape a skill.
const SHARED: &str = "c.kind<>'direct' AND NOT c.work AND (c.visibility='public' OR (SELECT count(*) FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id)>1)";

/// One earlier request of a routine job type.
#[derive(Debug, Clone)]
pub(crate) struct Example {
    pub job: String,
    pub chat: String,
    pub at_ms: i64,
    pub outcome: String,
    /// Read by two or more people: its text may be shown to the drafting model.
    pub shared: bool,
    pub request: String,
    pub answer: String,
    pub correction: Option<String>,
    pub steps: Vec<String>,
}

/// A routine job type and how it went.
#[derive(Debug, Clone)]
pub(crate) struct Routine {
    pub id: String,
    pub name: String,
    pub description: String,
    pub runs: i64,
    pub days: i64,
    pub accepted: i64,
    pub corrected: i64,
    /// Skills loaded in its runs.
    pub skills_used: Vec<String>,
    pub examples: Vec<Example>,
}

/// One of the agent's skills.
#[derive(Debug, Clone)]
pub(crate) struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub text: String,
    pub unused: bool,
}

/// Everything one review reads.
#[derive(Debug, Clone)]
pub(crate) struct ReviewWork {
    pub workspace: String,
    pub agent: String,
    pub name: String,
    pub role: String,
    pub requests: i64,
    pub job_types: i64,
    pub routine: Vec<Routine>,
    pub skills: Vec<AgentSkill>,
    /// Job types with an open proposal or one rejected in the last 30 days.
    pub blocked: Vec<String>,
    /// Members' names: never in a skill.
    pub names: Vec<String>,
}

/// A skill a review proposes.
#[derive(Debug, Clone)]
pub(crate) struct NewProposal {
    pub kind: &'static str,
    pub job_type: Option<String>,
    pub skill: Option<String>,
    pub name: String,
    pub description: String,
    pub files: Vec<SkillFile>,
    pub why: String,
    pub evidence: Vec<Value>,
    pub rehearsals: Vec<Value>,
}

impl PgStore {
    /// Managed agents (not assistants) with learning on, no review in 72 hours
    /// and at least ten sorted requests since their last one.
    pub(crate) async fn skill_reviews_due(
        &self,
        now_ms: i64,
        limit: i64,
    ) -> Result<Vec<(String, String)>> {
        if !ready(&self.db).await? {
            return Ok(vec![]);
        }
        rows(&self.db, "WITH last AS (SELECT a.workspace,a.id,(SELECT max(r.ran_ms) FROM relay_skill_reviews r WHERE r.workspace=a.workspace AND r.agent=a.id) AS ran
              FROM relay_shared_agents a LEFT JOIN relay_agent_learning l ON l.workspace=a.workspace AND l.agent=a.id
              WHERE a.revision>0 AND COALESCE(l.enabled,true) AND NOT EXISTS(SELECT 1 FROM relay_assistants s WHERE s.workspace=a.workspace AND s.agent=a.id))
            SELECT workspace,id FROM last WHERE COALESCE(ran,0)<$1-$2
              AND (SELECT count(*) FROM relay_request_job_types t WHERE t.workspace=last.workspace AND t.agent=last.id AND t.at_ms>COALESCE(last.ran,0))>=$3
            ORDER BY ran NULLS FIRST LIMIT $4", vec![now_ms.into(), EVERY_MS.into(), NEW_REQUESTS.into(), limit.into()])
            .await?
            .into_iter()
            .map(|r| Ok((r.try_get("", "workspace")?, r.try_get("", "id")?)))
            .collect()
    }

    /// What one agent's review reads, or why it is skipped (paused, budget).
    pub(crate) async fn skill_review_work(
        &self,
        w: &str,
        agent: &str,
        now_ms: i64,
    ) -> Result<std::result::Result<ReviewWork, &'static str>> {
        let tx = self.read_tx(w).await?;
        let Some(profile) = query(
            &tx,
            "SELECT name,role FROM relay_shared_agents WHERE workspace=$1 AND id=$2 AND revision>0",
            vec![w.into(), agent.into()],
        )
        .await?
        else {
            return Ok(Err("Not a managed agent"));
        };
        if super::super::settings::pause::paused(&tx, w).await? {
            return Ok(Err("Agents are paused"));
        }
        if super::super::model_budgets::budget_reached(&tx, w, agent)
            .await?
            .is_some()
        {
            return Ok(Err("The model budget is used up"));
        }
        let since = now_ms - LOOK_BACK_MS;
        let counts = query(&tx, "SELECT count(*) AS requests,count(DISTINCT job_type) AS types FROM relay_request_job_types WHERE workspace=$1 AND agent=$2 AND at_ms>$3",
            vec![w.into(), agent.into(), since.into()]).await?.context("Missing counts")?;
        let types = rows(&tx, "SELECT j.id,j.name,j.description,count(*) AS runs,
                count(DISTINCT floor(o.decided_ms/86400000.0)) AS days,
                count(*) FILTER (WHERE o.outcome='accepted') AS accepted,count(*) FILTER (WHERE o.outcome='corrected') AS corrected
            FROM relay_job_types j JOIN relay_request_job_types t ON t.workspace=j.workspace AND t.job_type=j.id
            JOIN relay_request_outcomes o ON o.workspace=t.workspace AND o.job=t.job
            WHERE j.workspace=$1 AND j.agent=$2 AND t.at_ms>$3 GROUP BY j.id,j.name,j.description
            HAVING count(*)>=$4 AND count(DISTINCT floor(o.decided_ms/86400000.0))>=$5 ORDER BY count(*) DESC LIMIT 5",
            vec![w.into(), agent.into(), since.into(), ROUTINE_RUNS.into(), ROUTINE_DAYS.into()]).await?;
        let mut routine = vec![];
        for t in types {
            let id: String = t.try_get("", "id")?;
            let skills_used = rows(&tx, "SELECT DISTINCT s.name FROM relay_request_job_types t JOIN relay_skill_loads l ON l.workspace=t.workspace AND l.job=t.job AND l.path IS NULL
                JOIN relay_skills s ON s.workspace=l.workspace AND s.id=l.skill WHERE t.workspace=$1 AND t.job_type=$2 AND t.at_ms>$3", vec![w.into(), id.clone().into(), since.into()])
                .await?.into_iter().map(|r| Ok(r.try_get("", "name")?)).collect::<Result<_>>()?;
            let examples = rows(&tx, &format!("SELECT t.job,m.chat,o.decided_ms,o.outcome,({SHARED}) AS shared,left(COALESCE(r.content,''),1200) AS request,
                    left(m.content,1500) AS answer,(SELECT left(f.content,600) FROM relay_chat_messages f WHERE f.workspace=o.workspace AND f.id=o.message) AS correction,
                    (SELECT COALESCE(jsonb_agg(x.title ORDER BY x.created),'[]'::jsonb) FROM relay_control_actions x WHERE x.workspace=t.workspace AND x.job=t.job) AS steps
                FROM relay_request_job_types t JOIN relay_request_outcomes o ON o.workspace=t.workspace AND o.job=t.job
                JOIN relay_chat_jobs j ON j.workspace=t.workspace AND j.reply=t.job
                JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
                JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
                LEFT JOIN relay_chat_messages r ON r.workspace=j.workspace AND r.id=j.request_message
                WHERE t.workspace=$1 AND t.job_type=$2 AND t.at_ms>$3
                ORDER BY array_position(ARRAY['corrected','accepted','followed_up','unknown'],o.outcome),o.decided_ms DESC LIMIT 6"), vec![w.into(), id.clone().into(), since.into()]).await?
                .into_iter()
                .map(|r| {
                    let steps: Value = r.try_get("", "steps")?;
                    Ok(Example {
                        job: r.try_get("", "job")?, chat: r.try_get("", "chat")?, at_ms: r.try_get("", "decided_ms")?, outcome: r.try_get("", "outcome")?,
                        shared: r.try_get("", "shared")?, request: r.try_get("", "request")?, answer: r.try_get("", "answer")?, correction: r.try_get("", "correction")?,
                        steps: steps.as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_owned)).collect(),
                    })
                })
                .collect::<Result<_>>()?;
            routine.push(Routine {
                id,
                name: t.try_get("", "name")?,
                description: t.try_get("", "description")?,
                runs: t.try_get("", "runs")?,
                days: t.try_get("", "days")?,
                accepted: t.try_get("", "accepted")?,
                corrected: t.try_get("", "corrected")?,
                skills_used,
                examples,
            });
        }
        let skills = rows(&tx, "SELECT s.id,s.name,s.description,s.version,COALESCE((SELECT f->>'text' FROM jsonb_array_elements(v.files) f WHERE f->>'path'='SKILL.md'),'') AS text,
                (s.created_ms<$3-$4 AND NOT EXISTS(SELECT 1 FROM relay_skill_loads l WHERE l.workspace=s.workspace AND l.skill=s.id AND l.at_ms>$3-$4)) AS unused
            FROM relay_agent_skills x JOIN relay_skills s ON s.workspace=x.workspace AND s.id=x.skill
            JOIN relay_skill_versions v ON v.workspace=s.workspace AND v.skill=s.id AND v.version=s.version
            WHERE x.workspace=$1 AND x.agent=$2 ORDER BY s.name", vec![w.into(), agent.into(), now_ms.into(), UNUSED_MS.into()]).await?
            .into_iter()
            .map(|r| Ok(AgentSkill { id: r.try_get("", "id")?, name: r.try_get("", "name")?, description: r.try_get("", "description")?, text: r.try_get("", "text")?,
                unused: r.try_get("", "unused")? }))
            .collect::<Result<_>>()?;
        let blocked = rows(&tx, "SELECT DISTINCT COALESCE(job_type,skill) AS id FROM relay_skill_proposals WHERE workspace=$1 AND agent=$2
            AND (state='proposed' OR (state='rejected' AND decided_ms>$3-$4))", vec![w.into(), agent.into(), now_ms.into(), REJECTED_WAIT_MS.into()]).await?
            .into_iter().filter_map(|r| r.try_get::<Option<String>>("", "id").ok().flatten()).collect();
        let names = rows(&tx, "SELECT DISTINCT u.display_name FROM relay_members p JOIN relay_users u ON u.id=p.user_id WHERE p.workspace=$1 AND u.display_name IS NOT NULL",
            vec![w.into()]).await?.into_iter().filter_map(|r| r.try_get::<String>("", "display_name").ok()).collect();
        tx.commit().await?;
        Ok(Ok(ReviewWork {
            workspace: w.into(),
            agent: agent.into(),
            name: profile.try_get("", "name")?,
            role: profile.try_get("", "role")?,
            requests: counts.try_get("", "requests")?,
            job_types: counts.try_get("", "types")?,
            routine,
            skills,
            blocked,
            names,
        }))
    }

    /// Saves one review and what it proposes. Returns the review's id.
    pub(crate) async fn save_skill_review(
        &self,
        work: &ReviewWork,
        result: &str,
        proposals: &[NewProposal],
        now_ms: i64,
    ) -> Result<String> {
        let w = &work.workspace;
        let id = uuid::Uuid::new_v4().to_string();
        let tx = self.workspace_tx(w).await?;
        let result: String = result.chars().take(400).collect();
        execute(&tx, "INSERT INTO relay_skill_reviews(workspace,id,agent,ran_ms,requests,job_types,routine,proposed,result) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            vec![w.clone().into(), id.clone().into(), work.agent.clone().into(), now_ms.into(), (work.requests as i32).into(), (work.job_types as i32).into(),
                (work.routine.len() as i32).into(), (proposals.len() as i32).into(), result.into()]).await?;
        for p in proposals {
            execute(&tx, "INSERT INTO relay_skill_proposals(workspace,id,agent,review,kind,job_type,skill,name,description,files,why,evidence,rehearsals,created_ms)
                VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
                vec![w.clone().into(), uuid::Uuid::new_v4().to_string().into(), work.agent.clone().into(), id.clone().into(), p.kind.into(), p.job_type.clone().into(),
                    p.skill.clone().into(), p.name.clone().into(), p.description.chars().take(1024).collect::<String>().into(), serde_json::to_value(&p.files)?.into(),
                    p.why.chars().take(600).collect::<String>().into(), json!(p.evidence).into(), json!(p.rehearsals).into(), now_ms.into()]).await?;
        }
        tx.commit().await?;
        Ok(id)
    }
}
