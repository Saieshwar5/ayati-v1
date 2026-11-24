//! Whether agents get better, in numbers: per agent for its owners and admins
//! (its page › Improving) and for the whole workspace for admins (Settings ›
//! Memory › Learning). Each measure is shown with the week before it.
use super::super::agent_profiles::managed_access;
use super::*;
use serde_json::json;

const WEEK_MS: i64 = 7 * 86_400_000;

/// The measures of one week, for one agent or (`agent` empty) every agent.
async fn week(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
    from: i64,
    to: i64,
) -> Result<Value> {
    let args = || -> Vec<sea_orm::Value> { vec![w.into(), agent.into(), from.into(), to.into()] };
    let o = query(db, "SELECT count(*) AS answers,count(*) FILTER (WHERE outcome='accepted') AS accepted,count(*) FILTER (WHERE outcome='corrected') AS corrected,
            count(*) FILTER (WHERE outcome='followed_up') AS followed_up FROM relay_request_outcomes
        WHERE workspace=$1 AND ($2='' OR agent=$2) AND decided_ms>=$3 AND decided_ms<$4", args()).await?.context("Missing outcomes")?;
    let m = query(db, "SELECT (SELECT count(*) FROM relay_memory_uses u WHERE u.workspace=$1 AND ($2='' OR u.agent=$2) AND u.action='searched' AND u.found=0 AND u.at_ms>=$3 AND u.at_ms<$4) AS missed,
            (SELECT count(*) FROM relay_memories n WHERE n.workspace=$1 AND n.source='agent' AND ($2='' OR n.author=$2) AND n.created_ms>=$3 AND n.created_ms<$4) AS learned,
            (SELECT count(*) FROM relay_memories n WHERE n.workspace=$1 AND n.waiting_reason='corrected' AND ($2='' OR n.agent=$2 OR n.author=$2) AND n.changed_ms>=$3 AND n.changed_ms<$4) AS held,
            (SELECT count(*) FROM relay_skill_proposals p WHERE p.workspace=$1 AND ($2='' OR p.agent=$2) AND p.kind<>'retire' AND p.created_ms>=$3 AND p.created_ms<$4) AS skills_proposed,
            (SELECT count(*) FROM relay_skill_proposals p WHERE p.workspace=$1 AND ($2='' OR p.agent=$2) AND p.kind<>'retire' AND p.state='approved' AND p.decided_ms>=$3 AND p.decided_ms<$4) AS skills_approved", args())
        .await?.context("Missing measures")?;
    let n = |row: &QueryResult, key: &str| row.try_get::<i64>("", key).unwrap_or(0);
    let answers = n(&o, "answers");
    let per100 = |count: i64| {
        if answers == 0 {
            Value::Null
        } else {
            json!(((count as f64) * 100.0 / answers as f64).round() as i64)
        }
    };
    Ok(
        json!({"answers":answers,"accepted":per100(n(&o, "accepted")),"corrected":per100(n(&o, "corrected")),"followed_up":per100(n(&o, "followed_up")),
        "missed":n(&m, "missed"),"learned":n(&m, "learned"),"held":n(&m, "held"),
        "skills_proposed":n(&m, "skills_proposed"),"skills_approved":n(&m, "skills_approved")}),
    )
}

/// The database's clock: what its rows were stamped with.
async fn db_now(db: &impl ConnectionTrait) -> Result<i64> {
    Ok(query(
        db,
        "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS now",
        vec![],
    )
    .await?
    .context("Missing clock")?
    .try_get("", "now")?)
}

impl PgStore {
    /// One agent's page › Improving: its weeks, reviews, job types, proposals and the skills it made.
    pub(crate) async fn agent_improving(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        managed_access(&tx, &user.id, w, agent, true).await?;
        if !ready(&tx).await? {
            tx.commit().await?;
            return Ok(json!({"available":false}));
        }
        let now = db_now(&tx).await?;
        let this = week(&tx, w, agent, now - WEEK_MS, now).await?;
        let last = week(&tx, w, agent, now - 2 * WEEK_MS, now - WEEK_MS).await?;
        let reviews = json_rows(&tx, "SELECT jsonb_build_object('id',id,'ran_ms',ran_ms,'requests',requests,'job_types',job_types,'routine',routine,'proposed',proposed,'result',result) AS data
            FROM relay_skill_reviews WHERE workspace=$1 AND agent=$2 ORDER BY ran_ms DESC LIMIT 10", vec![w.into(), agent.into()]).await?;
        let jobs = json_rows(&tx, "SELECT jsonb_build_object('id',j.id,'name',j.name,'runs',count(o.job),
                'accepted',count(*) FILTER (WHERE o.outcome='accepted'),'corrected',count(*) FILTER (WHERE o.outcome='corrected')) AS data
            FROM relay_job_types j JOIN relay_request_job_types t ON t.workspace=j.workspace AND t.job_type=j.id
            JOIN relay_request_outcomes o ON o.workspace=t.workspace AND o.job=t.job
            WHERE j.workspace=$1 AND j.agent=$2 AND t.at_ms>$3 GROUP BY j.id,j.name ORDER BY count(o.job) DESC LIMIT 12",
            vec![w.into(), agent.into(), (now - LOOK_BACK_MS).into()]).await?;
        // Each skill it made: how its job went in the 30 days before and since.
        let skills = json_rows(&tx, "SELECT jsonb_build_object('id',s.id,'name',s.name,'version',s.version,'created_ms',s.created_ms,
                'job',j.name,
                'before',(SELECT jsonb_build_object('runs',count(*),'corrected',count(*) FILTER (WHERE o.outcome='corrected')) FROM relay_request_job_types t
                  JOIN relay_request_outcomes o ON o.workspace=t.workspace AND o.job=t.job WHERE t.workspace=s.workspace AND t.job_type=p.job_type
                  AND o.decided_ms<s.created_ms AND o.decided_ms>=s.created_ms-$3),
                'since',(SELECT jsonb_build_object('runs',count(*),'corrected',count(*) FILTER (WHERE o.outcome='corrected')) FROM relay_request_job_types t
                  JOIN relay_request_outcomes o ON o.workspace=t.workspace AND o.job=t.job WHERE t.workspace=s.workspace AND t.job_type=p.job_type
                  AND o.decided_ms>=s.created_ms)) AS data
            FROM relay_skills s LEFT JOIN relay_skill_proposals p ON p.workspace=s.workspace AND p.created_skill=s.id AND p.kind='new'
            LEFT JOIN relay_job_types j ON j.workspace=p.workspace AND j.id=p.job_type
            WHERE s.workspace=$1 AND s.created_by_agent=$2 ORDER BY s.created_ms DESC LIMIT 20", vec![w.into(), agent.into(), LOOK_BACK_MS.into()]).await?;
        tx.commit().await?;
        Ok(
            json!({"available":true,"this_week":this,"last_week":last,"reviews":reviews,"jobs":jobs,"skills":skills,
            "proposals":self.skill_proposals(user, w, agent).await?}),
        )
    }

    /// Settings › Memory › Learning: the workspace's week and each agent's, for admins.
    pub(crate) async fn workspace_learning(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        manager(&tx, w, &user.id).await.map_err(|_| {
            anyhow::anyhow!("Only admins can see how the workspace's agents are learning")
        })?;
        if !ready(&tx).await? {
            tx.commit().await?;
            return Ok(json!({"available":false}));
        }
        let now = db_now(&tx).await?;
        let this = week(&tx, w, "", now - WEEK_MS, now).await?;
        let last = week(&tx, w, "", now - 2 * WEEK_MS, now - WEEK_MS).await?;
        let agents = rows(&tx, "SELECT DISTINCT o.agent,a.name FROM relay_request_outcomes o JOIN relay_shared_agents a ON a.workspace=o.workspace AND a.id=o.agent
            WHERE o.workspace=$1 AND o.decided_ms>=$2 ORDER BY a.name LIMIT 50", vec![w.into(), (now - 2 * WEEK_MS).into()]).await?;
        let mut each = vec![];
        for a in agents {
            let id: String = a.try_get("", "agent")?;
            each.push(json!({"id":id,"name":a.try_get::<String>("", "name")?,
                "this_week":week(&tx, w, &id, now - WEEK_MS, now).await?,"last_week":week(&tx, w, &id, now - 2 * WEEK_MS, now - WEEK_MS).await?}));
        }
        tx.commit().await?;
        Ok(json!({"available":true,"this_week":this,"last_week":last,"agents":each}))
    }
}
