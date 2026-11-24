//! Each agent's job types: the kinds of work it does, named from its requests.
//! Every finished request with an outcome is sorted into one, or a new one.
use super::*;

/// A request to sort, with the agent's job types so far.
#[derive(Debug, Clone)]
pub(crate) struct ToSort {
    pub workspace: String,
    pub job: String,
    pub agent: String,
    pub request: String,
    /// (id, name, description)
    pub types: Vec<(String, String, String)>,
}

/// Where a request goes: an existing job type, or a new one with a name.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Sorted {
    Existing(String),
    New { name: String, description: String },
}

impl PgStore {
    /// Requests of managed agents (not assistants) with an outcome and no job type, oldest first.
    pub(crate) async fn requests_to_sort(&self, limit: i64) -> Result<Vec<ToSort>> {
        if !ready(&self.db).await? {
            return Ok(vec![]);
        }
        let found = rows(&self.db, "SELECT o.workspace,o.job,o.agent,left(COALESCE(r.content,''),1000) AS request
            FROM relay_request_outcomes o JOIN relay_chat_jobs j ON j.workspace=o.workspace AND j.reply=o.job
            LEFT JOIN relay_chat_messages r ON r.workspace=j.workspace AND r.id=j.request_message
            LEFT JOIN relay_request_job_types t ON t.workspace=o.workspace AND t.job=o.job
            WHERE t.job IS NULL AND NOT EXISTS(SELECT 1 FROM relay_assistants a WHERE a.workspace=o.workspace AND a.agent=o.agent)
              AND COALESCE(r.content,'')<>'' AND o.decided_ms>(extract(epoch FROM clock_timestamp())*1000)::bigint-$2
            ORDER BY o.decided_ms LIMIT $1", vec![limit.into(), LOOK_BACK_MS.into()]).await?;
        let mut sorted = vec![];
        for row in found {
            let (w, agent): (String, String) =
                (row.try_get("", "workspace")?, row.try_get("", "agent")?);
            let types = rows(&self.db, "SELECT id,name,description FROM relay_job_types WHERE workspace=$1 AND agent=$2 ORDER BY created_ms LIMIT 60",
                vec![w.clone().into(), agent.clone().into()]).await?
                .into_iter()
                .map(|r| Ok((r.try_get("", "id")?, r.try_get("", "name")?, r.try_get("", "description")?)))
                .collect::<Result<_>>()?;
            sorted.push(ToSort {
                workspace: w,
                job: row.try_get("", "job")?,
                agent,
                request: row.try_get("", "request")?,
                types,
            });
        }
        Ok(sorted)
    }

    /// Files a request under a job type, creating it when new. A job type is never
    /// another agent's.
    pub(crate) async fn save_job_type(
        &self,
        item: &ToSort,
        sorted: &Sorted,
        confidence: f64,
    ) -> Result<String> {
        let w = &item.workspace;
        let tx = self.workspace_tx(w).await?;
        let id = match sorted {
            Sorted::Existing(id) => {
                let ok = query(
                    &tx,
                    "SELECT 1 AS ok FROM relay_job_types WHERE workspace=$1 AND id=$2 AND agent=$3",
                    vec![
                        w.clone().into(),
                        id.clone().into(),
                        item.agent.clone().into(),
                    ],
                )
                .await?
                .is_some();
                ensure!(ok, "Not one of this agent's job types");
                id.clone()
            }
            Sorted::New { name, description } => {
                let name: String = name
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(80)
                    .collect();
                ensure!(!name.is_empty(), "A job type needs a name");
                let description: String = description.chars().take(300).collect();
                let id = uuid::Uuid::new_v4().to_string();
                execute(&tx, "INSERT INTO relay_job_types(workspace,id,agent,name,description) VALUES($1,$2,$3,$4,$5)
                    ON CONFLICT(workspace,agent,lower(name)) DO NOTHING", vec![w.clone().into(), id.into(), item.agent.clone().into(), name.clone().into(), description.into()]).await?;
                query(&tx, "SELECT id FROM relay_job_types WHERE workspace=$1 AND agent=$2 AND lower(name)=lower($3)",
                    vec![w.clone().into(), item.agent.clone().into(), name.into()]).await?.context("Job type missing")?.try_get("", "id")?
            }
        };
        execute(&tx, "INSERT INTO relay_request_job_types(workspace,job,agent,job_type,confidence) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
            vec![w.clone().into(), item.job.clone().into(), item.agent.clone().into(), id.clone().into(), (confidence as f32).into()]).await?;
        tx.commit().await?;
        Ok(id)
    }
}
