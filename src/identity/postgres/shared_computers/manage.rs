//! Settings › Computers › Your computers: admins add a machine (a one-time
//! code for its connector), choose which agents use it, and remove it.
use super::{digest, listing};
use crate::identity::postgres::*;
use serde_json::json;

/// A one-time code works for this long.
const CODE_MS: i64 = 15 * 60_000;
/// Machines one workspace may add.
const MOST: i64 = 20;

async fn admin(tx: &DatabaseTransaction, w: &str, user: &User) -> Result<()> {
    ensure!(
        role(tx, w, &user.id).await? == "Admin",
        "Only admins add and change the workspace's computers"
    );
    Ok(())
}

/// A code people can type: four groups of four letters and digits.
fn code() -> String {
    let token = crate::identity::provider::random_token();
    let clean: String = token
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .filter(|c| !"0O1I".contains(*c))
        .take(16)
        .collect();
    clean
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

impl PgStore {
    /// The workspace's own computers, with their state and agents.
    pub(crate) async fn workspace_computers(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        if !super::ready(&tx).await? {
            tx.commit().await?;
            return Ok(json!({"computers":[],"can_change":admin}));
        }
        let now_ms: i64 = query(
            &tx,
            "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS now",
            vec![],
        )
        .await?
        .context("Clock")?
        .try_get("", "now")?;
        let rows = json_rows(&tx, "SELECT jsonb_build_object('id',c.id,'name',c.name,'connected',c.device IS NOT NULL,'platform',c.platform,
                'last_seen_ms',c.last_seen_ms,'revision',c.revision,'busy',c.held_by IS NOT NULL AND c.held_until>extract(epoch FROM clock_timestamp())::bigint,
                'code_expires_ms',CASE WHEN c.device IS NULL THEN c.code_expires_ms END,
                'agents',(SELECT coalesce(jsonb_agg(jsonb_build_object('id',a.agent,'name',s.name) ORDER BY s.name),'[]'::jsonb)
                  FROM relay_workspace_computer_agents a JOIN relay_shared_agents s ON s.workspace=a.workspace AND s.id=a.agent
                  WHERE a.workspace=c.workspace AND a.computer=c.id)) AS data
            FROM relay_workspace_computers c WHERE c.workspace=$1 AND NOT c.removed ORDER BY c.created_ms,c.id", vec![w.into()]).await?;
        tx.commit().await?;
        Ok(json!({"computers":listing(rows, now_ms),"can_change":admin}))
    }

    /// Adds a machine and returns its one-time code, shown once. `id` comes
    /// from the browser, so a repeat of the same request makes no second one.
    pub(crate) async fn add_workspace_computer(
        &self,
        user: &User,
        w: &str,
        id: &str,
        name: &str,
    ) -> Result<Value> {
        crate::identity::worker_control::model::uuid(id)?;
        let name = name.trim();
        ensure!(
            !name.is_empty() && name.len() <= 100 && !name.chars().any(char::is_control),
            "Give the computer a name of up to 100 characters, such as Office PC"
        );
        let tx = self.workspace_tx(w).await?;
        admin(&tx, w, user).await?;
        ensure!(
            super::ready(&tx).await?,
            "Run workspace-db migrate to add computers"
        );
        if query(
            &tx,
            "SELECT 1 AS found FROM relay_workspace_computers WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .is_some()
        {
            tx.commit().await?;
            return Ok(json!({"id":id,"code":null,"repeat":true}));
        }
        let count: i64 = query(&tx, "SELECT count(*) AS n FROM relay_workspace_computers WHERE workspace=$1 AND NOT removed", vec![w.into()])
            .await?.context("Count")?.try_get("", "n")?;
        ensure!(count < MOST, "A workspace can add up to 20 computers");
        let code = code();
        execute(&tx, "INSERT INTO relay_workspace_computers(workspace,id,name,code,code_expires_ms,created_by) VALUES($1,$2,$3,$4,(extract(epoch FROM clock_timestamp())*1000)::bigint+$5,$6)",
            vec![w.into(), id.into(), name.into(), digest(&code).into(), CODE_MS.into(), user.id.clone().into()]).await?;
        audit(&tx, &user.id, Some(w), "workspace_computer_added", id).await?;
        tx.commit().await?;
        Ok(json!({"id":id,"code":code,"expires_minutes":CODE_MS / 60_000}))
    }

    /// A new one-time code, for a machine whose connector must sign in again
    /// (a new PC, or a lost key). The old connector key stops at once.
    pub(crate) async fn new_computer_code(
        &self,
        user: &User,
        w: &str,
        id: &str,
        revision: i64,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        admin(&tx, w, user).await?;
        let code = code();
        let changed = execute(&tx, "UPDATE relay_workspace_computers SET code=$3,code_expires_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint+$4,device=NULL,last_seen_ms=NULL,revision=revision+1
            WHERE workspace=$1 AND id=$2 AND revision=$5 AND NOT removed", vec![w.into(), id.into(), digest(&code).into(), CODE_MS.into(), revision.into()]).await?;
        ensure!(
            changed == 1,
            "This computer changed; reload before trying again"
        );
        audit(&tx, &user.id, Some(w), "workspace_computer_new_code", id).await?;
        tx.commit().await?;
        Ok(json!({"id":id,"code":code,"expires_minutes":CODE_MS / 60_000}))
    }

    /// Chooses the agents that use this machine. An agent uses one computer
    /// at most, so choosing it here moves it from any other machine.
    pub(crate) async fn choose_computer_agents(
        &self,
        user: &User,
        w: &str,
        id: &str,
        revision: i64,
        agents: &[String],
    ) -> Result<Value> {
        ensure!(agents.len() <= 50, "Choose at most 50 agents");
        let tx = self.workspace_tx(w).await?;
        admin(&tx, w, user).await?;
        let changed = execute(&tx, "UPDATE relay_workspace_computers SET revision=revision+1 WHERE workspace=$1 AND id=$2 AND revision=$3 AND NOT removed",
            vec![w.into(), id.into(), revision.into()]).await?;
        ensure!(changed == 1, "This computer changed; reload before saving");
        execute(
            &tx,
            "DELETE FROM relay_workspace_computer_agents WHERE workspace=$1 AND computer=$2",
            vec![w.into(), id.into()],
        )
        .await?;
        for agent in agents {
            super::super::agent_profiles::stored(&tx, w, agent).await?;
            execute(&tx, "INSERT INTO relay_workspace_computer_agents(workspace,agent,computer,chosen_by) VALUES($1,$2,$3,$4)
                ON CONFLICT(workspace,agent) DO UPDATE SET computer=excluded.computer,chosen_by=excluded.chosen_by,chosen_ms=excluded.chosen_ms",
                vec![w.into(), agent.clone().into(), id.into(), user.id.clone().into()]).await?;
        }
        audit(&tx, &user.id, Some(w), "workspace_computer_agents", id).await?;
        tx.commit().await?;
        Ok(json!({"id":id,"agents":agents}))
    }

    /// Removes a machine: new work stops at once and its connector is refused.
    /// Work already sent to it finishes or is abandoned; nothing is retried.
    pub(crate) async fn remove_workspace_computer(
        &self,
        user: &User,
        w: &str,
        id: &str,
        revision: i64,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        admin(&tx, w, user).await?;
        let changed = execute(&tx, "UPDATE relay_workspace_computers SET removed=true,device=NULL,code=NULL,revision=revision+1 WHERE workspace=$1 AND id=$2 AND revision=$3 AND NOT removed",
            vec![w.into(), id.into(), revision.into()]).await?;
        ensure!(
            changed == 1,
            "This computer changed; reload before removing it"
        );
        execute(
            &tx,
            "DELETE FROM relay_workspace_computer_agents WHERE workspace=$1 AND computer=$2",
            vec![w.into(), id.into()],
        )
        .await?;
        execute(&tx, "UPDATE relay_workspace_computer_calls SET state='abandoned',done_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND computer=$2 AND state='queued'",
            vec![w.into(), id.into()]).await?;
        audit(&tx, &user.id, Some(w), "workspace_computer_removed", id).await?;
        tx.commit().await?;
        Ok(json!({"id":id,"removed":true}))
    }
}
