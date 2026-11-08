//! Cuckoo Control → Computers (designs/agent-computers 6): every agent computer with
//! its provider, state and this month's awake time, charge and cost, by provider, and
//! the switch that pauses them all. Operators only; workspaces never see providers.
use super::*;

impl PgStore {
    pub(crate) async fn operator_computers(&self) -> Result<Value> {
        let paused = self.platform_setting(settings::COMPUTERS_PAUSED).await? == json!(true);
        let sizes = match &self.computers {
            Some(computers) => match computers.file() {
                Ok(file) => json!(file.sizes.iter().map(|s| json!({"size": s.size, "cpu": s.cpu, "memory_gb": s.memory_gb,
                    "usd_per_hour": s.usd_per_hour,
                    "routes": s.routes.iter().map(|r| json!({"provider": r.provider, "image": r.image, "usd_per_hour": r.usd_per_hour})).collect::<Vec<_>>()})).collect::<Vec<_>>()),
                Err(error) => json!({"error": error.to_string()}),
            },
            None => Value::Null,
        };
        if !super::super::model_budgets::present(&self.db, "relay_computers").await? {
            return Ok(
                json!({"configured": self.computers.is_some(), "paused": paused, "sizes": sizes, "providers": [], "computers": []}),
            );
        }
        let month = "date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'";
        let providers = json_rows(&self.db, &format!("SELECT jsonb_build_object('provider',p.provider,'computers',p.computers,'awake',p.awake,'problems',p.problems,
              'seconds',COALESCE(m.seconds,0),'charge',COALESCE(m.charge,0)/1e6,'cost',COALESCE(m.cost,0)/1e6) AS data
            FROM (SELECT provider,count(*) AS computers,count(*) FILTER (WHERE state='awake') AS awake,count(*) FILTER (WHERE problem IS NOT NULL) AS problems
                  FROM relay_computers GROUP BY provider) p
            LEFT JOIN (SELECT provider,sum(seconds)::bigint AS seconds,sum(charge_micro)::bigint AS charge,sum(cost_micro)::bigint AS cost
                  FROM relay_computer_sessions WHERE woke_at>=extract(epoch FROM {month})::bigint GROUP BY provider) m ON m.provider=p.provider
            ORDER BY p.computers DESC"), vec![]).await?;
        let computers = json_rows(&self.db, &format!("SELECT jsonb_build_object('workspace',c.workspace,'workspace_name',COALESCE(w.name,c.workspace),
              'agent',c.agent,'agent_name',COALESCE(a.name,'Removed agent'),'size',c.size,'provider',c.provider,'provider_id',c.provider_id,
              'state',c.state,'awake_since',c.awake_since,'last_used',c.last_used,'problem',c.problem,
              'in_use',c.held_by IS NOT NULL AND c.held_until>extract(epoch FROM now())::bigint,
              'seconds',COALESCE(m.seconds,0),'charge',COALESCE(m.charge,0)/1e6,'cost',COALESCE(m.cost,0)/1e6) AS data
            FROM relay_computers c
            LEFT JOIN relay_workspaces w ON w.id=c.workspace
            LEFT JOIN relay_shared_agents a ON a.workspace=c.workspace AND a.id=c.agent
            LEFT JOIN LATERAL (SELECT sum(seconds)::bigint AS seconds,sum(charge_micro)::bigint AS charge,sum(cost_micro)::bigint AS cost
                  FROM relay_computer_sessions s WHERE s.workspace=c.workspace AND s.agent=c.agent AND s.woke_at>=extract(epoch FROM {month})::bigint) m ON TRUE
            ORDER BY (c.state='awake') DESC, c.problem IS NOT NULL DESC, c.last_used DESC LIMIT 300"), vec![]).await?;
        Ok(
            json!({"configured": self.computers.is_some(), "paused": paused, "sizes": sizes, "providers": providers, "computers": computers}),
        )
    }
}
