//! Saving a connected system: the connection, its approved server, the rule the
//! person chose for each tool and the agent's switch, in one transaction.
use super::{NewSystem, Reviewed, ToolChoice, tiles};
use crate::identity::postgres::*;
use serde_json::json;

/// The agent's owner or an admin may connect or change its systems.
pub(super) async fn may_change(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
) -> Result<()> {
    super::super::agent_profiles::managed_access(tx, &user.id, w, agent, true)
        .await
        .map(|_| ())
        .map_err(|_| {
            anyhow::anyhow!("Only the agent's owner or an admin can connect or change its systems")
        })
}

/// The saved system for `agent`, locked for this change.
pub(super) async fn saved(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    id: &str,
) -> Result<Value> {
    json_rows(tx, "SELECT jsonb_build_object('name',s.name,'removed',s.removed,'revision',s.revision,'server',c.server,
            'connection_revision',c.revision,'enabled',c.enabled,'binding',c.binding) AS data
        FROM relay_connected_systems s JOIN relay_control_connections c ON c.workspace=s.workspace AND c.id=s.connection
        WHERE s.workspace=$1 AND s.agent=$2 AND s.connection=$3 FOR UPDATE OF s",
        vec![w.into(), agent.into(), id.into()]).await?
        .into_iter()
        .next()
        .filter(|s| s["removed"] != true)
        .context("This system is no longer connected to the agent")
}

/// Sets the rule for every reviewed tool. The workspace rule is the person's
/// choice: the system exists only because they connected it, and an admin can
/// still make any rule stricter in Apps.
async fn save_rules(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    connection: &str,
    tools: &[ToolChoice],
) -> Result<()> {
    execute(
        tx,
        "DELETE FROM relay_connector_rules WHERE workspace=$1 AND connection=$2",
        vec![w.into(), connection.into()],
    )
    .await?;
    for tool in tools {
        execute(tx, "INSERT INTO relay_connector_rules(workspace,connection,operation,mode,needs_other,revision,updated_by,updated_at) VALUES($1,$2,$3,$4,false,nextval('relay_control_revision'),$5,$6)",
            vec![w.into(), connection.into(), tool.name.clone().into(), tool.rule.clone().into(), user.id.clone().into(), now().into()]).await?;
    }
    Ok(())
}

impl PgStore {
    /// Connects a reviewed system to `agent`. Repeating the same request returns
    /// the saved system; the same id with a different request is refused.
    pub(crate) async fn connect_system(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &NewSystem,
        reviewed: &Reviewed,
    ) -> Result<Value> {
        input.validate()?;
        let hash = input.request_hash()?;
        let tx = self.workspace_tx(w).await?;
        may_change(&tx, user, w, agent).await?;
        ensure!(
            super::ready(&tx).await?,
            "Run workspace-db migrate to connect systems"
        );
        if let Some(row) = query(&tx, "SELECT agent,request_hash FROM relay_connected_systems WHERE workspace=$1 AND connection=$2",
            vec![w.into(), input.id.clone().into()]).await?
        {
            ensure!(
                row.try_get::<String>("", "agent")? == agent
                    && row.try_get::<String>("", "request_hash")? == hash,
                "This connection id belongs to a different request"
            );
            let systems = tiles(&tx, w, agent).await?;
            tx.commit().await?;
            return systems
                .into_iter()
                .find(|s| s["id"] == input.id.as_str())
                .context("This system was removed");
        }
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_connected_systems WHERE workspace=$1 AND NOT removed",
            vec![w.into()],
        )
        .await?
        .context("Count")?
        .try_get("", "n")?;
        ensure!(count < 50, "A workspace can connect up to 50 systems");
        let server = crate::identity::connectors::systems::server_name(&input.id);
        let name = input.name.trim();
        execute(&tx, "INSERT INTO relay_control_systems(workspace,server,approved,revision) VALUES($1,$2,true,1)",
            vec![w.into(), server.clone().into()]).await?;
        execute(&tx, "INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint,provider,binding,connected_by,connected_at) VALUES($1,$2,$3,$4,$5,'system',$6,$7,$8)",
            vec![w.into(), input.id.clone().into(), server.clone().into(), reviewed.definition(&server, name).into(),
                reviewed.fingerprint(&server, name)?.into(), reviewed.binding.clone().into(), user.id.clone().into(), now().into()]).await?;
        save_rules(&tx, user, w, &input.id, &input.tools).await?;
        execute(&tx, "INSERT INTO relay_agent_connections(workspace,agent,connection,revision,enabled_by,at) VALUES($1,$2,$3,nextval('relay_control_revision'),$4,$5)",
            vec![w.into(), agent.into(), input.id.clone().into(), user.id.clone().into(), now().into()]).await?;
        execute(&tx, "INSERT INTO relay_connected_systems(workspace,connection,agent,name,tools,request_hash,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)",
            vec![w.into(), input.id.clone().into(), agent.into(), name.into(), serde_json::to_value(&input.tools)?.into(), hash.into(), user.id.clone().into()]).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            "connected_system_added",
            &format!("{agent}/{}", input.id),
        )
        .await?;
        let systems = tiles(&tx, w, agent).await?;
        tx.commit().await?;
        systems
            .into_iter()
            .find(|s| s["id"] == input.id.as_str())
            .context("The system was not saved")
    }

    /// The saved address and binding, for reviewing tools again with the saved key.
    pub(crate) async fn system_binding(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<(String, Value)> {
        let tx = self.workspace_tx(w).await?;
        may_change(&tx, user, w, agent).await?;
        let system = saved(&tx, w, agent, id).await?;
        tx.commit().await?;
        Ok((
            system["server"].as_str().unwrap_or_default().to_owned(),
            system["binding"].clone(),
        ))
    }

    /// Saves a new review of the system's tools (after a tool changed, or to
    /// switch tools on or off). Pending approvals ask again; running work is fenced.
    pub(crate) async fn review_system(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        review: &super::SystemReview,
        reviewed: &Reviewed,
    ) -> Result<Value> {
        let (revision, tools) = (review.revision, review.tools.as_slice());
        super::check_tools(tools)?;
        let tx = self.workspace_tx(w).await?;
        may_change(&tx, user, w, agent).await?;
        let system = saved(&tx, w, agent, id).await?;
        ensure!(
            system["revision"] == revision,
            "This system changed; reload before saving"
        );
        let server = system["server"].as_str().context("Missing server")?;
        let name = system["name"].as_str().context("Missing name")?;
        execute(&tx, "UPDATE relay_control_connections SET definition=$3,fingerprint=$4,binding=$5,revision=revision+1 WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into(), reviewed.definition(server, name).into(), reviewed.fingerprint(server, name)?.into(), reviewed.binding.clone().into()]).await?;
        save_rules(&tx, user, w, id, tools).await?;
        super::super::control_rules::invalidate(&tx, w, id, None, None).await?;
        execute(&tx, "UPDATE relay_connected_systems SET tools=$3,revision=revision+1,reviewed_by=$4,reviewed_ms=$5 WHERE workspace=$1 AND connection=$2",
            vec![w.into(), id.into(), serde_json::to_value(tools)?.into(), user.id.clone().into(), now().into()]).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            "connected_system_reviewed",
            &format!("{agent}/{id}"),
        )
        .await?;
        let systems = tiles(&tx, w, agent).await?;
        tx.commit().await?;
        systems
            .into_iter()
            .find(|s| s["id"] == id)
            .context("The system was not saved")
    }

    /// Switches a system off (nothing is sent to it, waiting approvals stop) or
    /// on again, or removes it for good.
    pub(crate) async fn change_system(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        revision: i64,
        change: &str,
    ) -> Result<Value> {
        ensure!(
            ["on", "off", "remove"].contains(&change),
            "Choose on, off or remove"
        );
        let tx = self.workspace_tx(w).await?;
        may_change(&tx, user, w, agent).await?;
        let system = saved(&tx, w, agent, id).await?;
        ensure!(
            system["revision"] == revision,
            "This system changed; reload before saving"
        );
        let enabled = change == "on";
        execute(&tx, "UPDATE relay_control_connections SET enabled=$3,revision=revision+1 WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into(), enabled.into()]).await?;
        if !enabled {
            super::super::control_rules::invalidate(&tx, w, id, None, None).await?;
        }
        execute(&tx, "UPDATE relay_connected_systems SET revision=revision+1,removed=$3 WHERE workspace=$1 AND connection=$2",
            vec![w.into(), id.into(), (change == "remove").into()]).await?;
        if change == "remove" {
            execute(
                &tx,
                "DELETE FROM relay_agent_connections WHERE workspace=$1 AND connection=$2",
                vec![w.into(), id.into()],
            )
            .await?;
        }
        audit(
            &tx,
            &user.id,
            Some(w),
            &format!("connected_system_{change}"),
            &format!("{agent}/{id}"),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"id":id,"state":change}))
    }
}
