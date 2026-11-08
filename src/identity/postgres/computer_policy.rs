//! Schema 105: each workspace's limits on agent computers (Settings → Computers). Every
//! member may read them; admins change them. See runtime/agent-computers.md.
use super::*;
use crate::identity::agents::computer_policy::ComputerPolicy;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;

pub(super) struct ComputerPolicyMigration;
impl MigrationName for ComputerPolicyMigration {
    fn name(&self) -> &str {
        "m20261108_000105_computer_policies"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ComputerPolicyMigration {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(
                "CREATE TABLE relay_computer_policies (
                  workspace TEXT PRIMARY KEY REFERENCES relay_workspaces(id),
                  policy JSONB NOT NULL CHECK(octet_length(policy::text) <= 20000),
                  updated_by TEXT NOT NULL,
                  updated_ms BIGINT NOT NULL);",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Computer policies require a forward migration".into(),
        ))
    }
}

/// The workspace's limits; the defaults allow everything until an admin sets them.
pub(super) async fn workspace_policy(db: &impl ConnectionTrait, w: &str) -> Result<ComputerPolicy> {
    if !super::model_budgets::present(db, "relay_computer_policies").await? {
        return Ok(ComputerPolicy::default());
    }
    let row = query(
        db,
        "SELECT policy FROM relay_computer_policies WHERE workspace=$1",
        vec![w.into()],
    )
    .await?;
    Ok(match row {
        Some(row) => serde_json::from_value(row.try_get("", "policy")?)?,
        None => ComputerPolicy::default(),
    })
}

impl PgStore {
    pub(crate) async fn computer_policy(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let policy = workspace_policy(&tx, w).await?;
        // Admins also see each agent's computer and this month's use.
        let computers = if admin && super::model_budgets::present(&tx, "relay_computers").await? {
            json_rows(&tx, "WITH zone AS (SELECT COALESCE(to_jsonb(w)->>'timezone','UTC') AS z FROM relay_workspaces w WHERE w.id=$1),
                  start AS (SELECT extract(epoch FROM (date_trunc('month', now() AT TIME ZONE z) AT TIME ZONE z))::bigint AS s FROM zone)
                SELECT jsonb_build_object('agent',c.agent,'name',COALESCE(a.name,'Removed agent'),'size',c.size,'state',c.state,
                  'in_use',c.held_by IS NOT NULL AND c.held_until>extract(epoch FROM now())::bigint,'last_used',c.last_used,
                  'waiting',(SELECT count(*) FROM relay_computer_waits q WHERE q.workspace=c.workspace AND q.agent=c.agent),
                  'seconds',COALESCE(m.seconds,0),'charge_usd',COALESCE(m.charge,0)/1e6) AS data
                FROM relay_computers c CROSS JOIN start
                LEFT JOIN relay_shared_agents a ON a.workspace=c.workspace AND a.id=c.agent
                LEFT JOIN LATERAL (SELECT sum(CASE WHEN slept_at IS NULL THEN extract(epoch FROM now())::bigint-woke_at ELSE seconds END)::bigint AS seconds,
                    sum(charge_micro)::bigint AS charge FROM relay_computer_sessions s
                  WHERE s.workspace=c.workspace AND s.agent=c.agent AND s.woke_at>=start.s) m ON TRUE
                WHERE c.workspace=$1 ORDER BY COALESCE(m.charge,0) DESC, c.agent LIMIT 200", vec![w.into()]).await?
        } else {
            vec![]
        };
        tx.commit().await?;
        Ok(
            json!({"policy": policy, "can_change": admin, "configured": self.computers.is_some(), "computers": computers}),
        )
    }

    pub(crate) async fn save_computer_policy(
        &self,
        user: &User,
        w: &str,
        policy: &ComputerPolicy,
    ) -> Result<Value> {
        policy.validate()?;
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let ms = now() * 1000;
        execute(&tx, "INSERT INTO relay_computer_policies(workspace,policy,updated_by,updated_ms) VALUES($1,$2,$3,$4)
            ON CONFLICT(workspace) DO UPDATE SET policy=EXCLUDED.policy,updated_by=EXCLUDED.updated_by,updated_ms=EXCLUDED.updated_ms",
            vec![w.into(), serde_json::to_value(policy)?.into(), user.id.clone().into(), ms.into()]).await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "computer_policy_changed",
            w,
            if policy.allowed {
                "Changed the limits on agents' computers"
            } else {
                "Switched off agents' computers for the workspace"
            },
        )
        .await?;
        tx.commit().await?;
        self.computer_policy(user, w).await
    }
}
