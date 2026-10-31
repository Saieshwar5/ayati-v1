//! "Report a problem" (D232 slice 5): reports go to the Cuckoo team, not to the
//! workspace. Operators list them with `ayati workspace-db problem-reports`.
use super::*;
use serde::Deserialize;
use serde_json::json;

const TOPICS: [&str; 6] = [
    "agent",
    "chat",
    "connections",
    "library",
    "billing",
    "other",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProblemReport {
    /// Chosen by the browser so a retried send stores one report.
    pub id: String,
    pub topic: String,
    pub body: String,
    /// Browser and page facts, never message contents; omitted when unticked.
    pub technical: Option<Value>,
}

impl ProblemReport {
    fn validate(&self) -> Result<()> {
        ensure!(uuid::Uuid::parse_str(&self.id).is_ok(), "Invalid report");
        ensure!(
            TOPICS.contains(&self.topic.as_str()),
            "Choose what the problem is about"
        );
        let body = self.body.trim();
        ensure!(
            !body.is_empty() && body.chars().count() <= 4000,
            "Describe the problem in up to 4,000 characters"
        );
        if let Some(technical) = &self.technical {
            ensure!(
                technical.is_object() && technical.to_string().len() <= 2000,
                "Technical details are too long"
            );
        }
        Ok(())
    }
}

impl PgStore {
    pub(crate) async fn report_problem(
        &self,
        user: &User,
        w: &str,
        input: &ProblemReport,
    ) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        let role = role(&tx, w, &user.id).await?;
        let technical = input.technical.clone().map(|mut t| {
            t["role"] = json!(role);
            t
        });
        let saved = execute(&tx, "INSERT INTO relay_problem_reports(id,workspace,reporter,topic,body,technical) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(id) DO NOTHING",
            vec![input.id.clone().into(), w.into(), user.id.clone().into(), input.topic.clone().into(), input.body.trim().into(), technical.into()]).await?;
        if saved == 0 {
            // A retry with the same id must be the same report from the same person.
            let same = query(&tx, "SELECT 1 AS same FROM relay_problem_reports WHERE id=$1 AND workspace=$2 AND reporter=$3 AND topic=$4 AND body=$5",
                vec![input.id.clone().into(), w.into(), user.id.clone().into(), input.topic.clone().into(), input.body.trim().into()]).await?;
            ensure!(same.is_some(), "Report ID already used");
        }
        tx.commit().await?;
        if saved == 1 {
            tracing::info!(report = %input.id, workspace = %w, topic = %input.topic, "Problem reported");
        }
        Ok(json!({"id": input.id, "saved": true}))
    }

    /// The newest reports for the operator command line.
    pub async fn problem_reports(&self, limit: i64) -> Result<Vec<Value>> {
        json_rows(&self.db, "SELECT jsonb_build_object('id',r.id,'workspace',r.workspace,'reporter',COALESCE(u.email,r.reporter),
            'topic',r.topic,'body',r.body,'technical',r.technical,'created_ms',r.created_ms) AS data
            FROM relay_problem_reports r LEFT JOIN relay_users u ON u.id=r.reporter ORDER BY r.created_ms DESC LIMIT $1",
            vec![limit.into()]).await
    }
}
