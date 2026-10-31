//! The workspace rule for each connector action, set by admins (D231 slice 4):
//! Allow, Ask first or Block, and whether someone other than the requester must
//! approve. It is the loosest any agent may use.
use super::*;
use crate::identity::control::connections_model::ConnectorRule;

/// New connectors start safe: reading is allowed, changes ask first.
pub(super) fn default_mode(effect: &str) -> &'static str {
    if effect == "read" { "allow" } else { "ask" }
}

pub(super) fn label(mode: &str) -> &'static str {
    match mode {
        "allow" => "Allow",
        "ask" => "Ask first",
        _ => "Block",
    }
}

/// The admin's rule for one action, or its safe default.
pub(super) async fn workspace_mode(
    db: &impl ConnectionTrait,
    w: &str,
    connection: &str,
    operation: &str,
) -> Result<String> {
    let row = query(db,"SELECT op->>'effect' AS effect,w.mode FROM relay_control_connections c CROSS JOIN LATERAL jsonb_array_elements(c.definition->'operations') op LEFT JOIN relay_connector_rules w ON w.workspace=c.workspace AND w.connection=c.id AND w.operation=op->>'id' WHERE c.workspace=$1 AND c.id=$2 AND op->>'id'=$3",vec![w.into(),connection.into(),operation.into()]).await?.context("Unsupported operation")?;
    Ok(match row.try_get::<Option<String>>("", "mode")? {
        Some(mode) => mode,
        None => default_mode(&row.try_get::<String>("", "effect")?).to_string(),
    })
}

impl PgStore {
    /// The connectors any member may give a new agent, with each action's
    /// workspace rule (create step 4). Nothing about other agents is shown.
    pub(crate) async fn control_available(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let connections = json_rows(
            &tx,
            include_str!("agent_tools.sql"),
            vec![w.into(), "".into()],
        )
        .await?;
        tx.commit().await?;
        Ok(serde_json::json!({"connections":connections}))
    }
    /// Saves one action's workspace rule. Its new revision moves every agent's
    /// effective revision for the action forward, so waiting approvals ask again.
    pub(crate) async fn control_connector_rule(
        &self,
        user: &User,
        w: &str,
        input: &ConnectorRule,
    ) -> Result<()> {
        ensure!(
            ["allow", "ask", "block"].contains(&input.mode.as_str()),
            "Invalid rule"
        );
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        workspace_mode(&tx, w, &input.connection, &input.operation).await?;
        let old = query(&tx,"SELECT mode,needs_other,revision FROM relay_connector_rules WHERE workspace=$1 AND connection=$2 AND operation=$3",vec![w.into(),input.connection.clone().into(),input.operation.clone().into()]).await?;
        let actual = old
            .as_ref()
            .map(|r| r.try_get::<i64>("", "revision"))
            .transpose()?
            .unwrap_or(0);
        if let Some(row) = &old
            && actual != input.revision
        {
            // An exact retry after a lost response changes nothing.
            ensure!(
                row.try_get::<String>("", "mode")? == input.mode
                    && row.try_get::<bool>("", "needs_other")? == input.needs_other,
                "Settings changed; reload before saving"
            );
            tx.commit().await?;
            return Ok(());
        }
        ensure!(
            actual == input.revision,
            "Settings changed; reload before saving"
        );
        execute(&tx,"INSERT INTO relay_connector_rules(workspace,connection,operation,mode,needs_other,revision,updated_by,updated_at) VALUES($1,$2,$3,$4,$5,nextval('relay_control_revision'),$6,$7) ON CONFLICT(workspace,connection,operation) DO UPDATE SET mode=excluded.mode,needs_other=excluded.needs_other,revision=excluded.revision,updated_by=excluded.updated_by,updated_at=excluded.updated_at",vec![w.into(),input.connection.clone().into(),input.operation.clone().into(),input.mode.clone().into(),input.needs_other.into(),user.id.clone().into(),now().into()]).await?;
        super::control_rules::invalidate(&tx, w, &input.connection, None, Some(&input.operation))
            .await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            "control_connector_rule_saved",
            &format!("{}/{}", input.connection, input.operation),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
