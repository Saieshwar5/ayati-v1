//! Who can talk to an agent, its managers and its owner (D231 slice 2, D233).
//! "Who can talk" is `everyone`, `selected` (listed in `relay_chat_access`) or
//! `only_me`; `relay_agent_can_talk` in schema 45 is the single definition.
use super::*;
use crate::identity::agents::config::SaveAgent;
use serde_json::json;

/// Whether schema 45 is applied. Upgrade tests run this code on older schemas,
/// where private/workspace agents and per-person grants still decide.
pub(super) async fn exists(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_agent_managers') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Schema unavailable")?
    .try_get("", "present")?)
}

/// Older rows name `scope` instead of `audience`; callers read `audience` only.
pub(super) fn normalize(agent: &mut Value) {
    if agent.get("audience").is_none() {
        let private = agent["scope"] == "private";
        agent["audience"] = json!(if private { "only_me" } else { "selected" });
    }
}

pub(super) async fn can_talk(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
    user: &str,
) -> Result<bool> {
    let sql = if exists(db).await? {
        "SELECT relay_agent_can_talk($1,$2,$3) AS talk"
    } else {
        "SELECT EXISTS(SELECT 1 FROM relay_shared_agents a JOIN relay_members m ON m.workspace=a.workspace AND m.user_id=$3
          WHERE a.workspace=$1 AND a.id=$2 AND (a.owner_id=$3 OR (a.scope<>'private' AND (m.role IN ('Admin','Owner')
           OR EXISTS(SELECT 1 FROM relay_chat_access g WHERE g.workspace=$1 AND g.agent=$2 AND g.user_id=$3))))) AS talk"
    };
    Ok(query(db, sql, vec![w.into(), agent.into(), user.into()])
        .await?
        .context("Agent unavailable")?
        .try_get("", "talk")?)
}

/// Only-me agents cannot be in channels or groups, where others could ask them.
pub(super) async fn not_in_channels(db: &impl ConnectionTrait, w: &str, agent: &str) -> Result<()> {
    ensure!(
        query(
            db,
            "SELECT x.chat FROM relay_chat_agents x JOIN relay_chats c ON c.workspace=x.workspace AND c.id=x.chat WHERE x.workspace=$1 AND x.agent=$2 AND c.kind<>'direct' LIMIT 1",
            vec![w.into(), agent.into()],
        )
        .await?
        .is_none(),
        "Remove this agent from its channels before choosing Only me"
    );
    Ok(())
}

async fn listed(db: &impl ConnectionTrait, sql: &str, w: &str, agent: &str) -> Result<Vec<String>> {
    rows(db, sql, vec![w.into(), agent.into()])
        .await?
        .iter()
        .map(|row| Ok(row.try_get("", "user_id")?))
        .collect()
}

async fn members(db: &impl ConnectionTrait, w: &str, people: &[String]) -> Result<Vec<String>> {
    let mut wanted = people.to_vec();
    wanted.sort();
    wanted.dedup();
    for person in &wanted {
        role(db, w, person)
            .await
            .context("Everyone you choose must be in this workspace")?;
    }
    Ok(wanted)
}

/// Saves the selected people and managers that `input` names. A new agent's
/// first manager is its creator. Reports what changed and whether someone lost access.
pub(super) async fn save_lists(
    tx: &impl ConnectionTrait,
    actor: &str,
    w: &str,
    input: &SaveAgent,
    creating: bool,
) -> Result<super::agent_changes::ListChanges> {
    let mut changes = super::agent_changes::ListChanges::default();
    if !exists(tx).await? {
        return Ok(changes);
    }
    let agent = input.id.as_str();
    if let Some(people) = &input.people {
        let wanted = members(tx, w, people).await?;
        let current = listed(tx, "SELECT user_id FROM relay_chat_access WHERE workspace=$1 AND agent=$2 ORDER BY user_id", w, agent).await?;
        if wanted != current {
            changes.people = true;
            changes.narrowed = current.iter().any(|person| !wanted.contains(person));
            execute(
                tx,
                "DELETE FROM relay_chat_access WHERE workspace=$1 AND agent=$2",
                vec![w.into(), agent.into()],
            )
            .await?;
            for person in &wanted {
                execute(
                    tx,
                    "INSERT INTO relay_chat_access(workspace,user_id,agent) VALUES($1,$2,$3)",
                    vec![w.into(), person.clone().into(), agent.into()],
                )
                .await?;
            }
        }
    }
    let managers = match &input.managers {
        Some(managers) => {
            ensure!(!managers.is_empty(), "Choose at least one manager");
            Some(members(tx, w, managers).await?)
        }
        None if creating => Some(vec![actor.to_string()]),
        None => None,
    };
    if let Some(wanted) = managers {
        let current = listed(tx, "SELECT user_id FROM relay_agent_managers WHERE workspace=$1 AND agent=$2 ORDER BY user_id", w, agent).await?;
        if wanted != current {
            changes.managers = !creating;
            execute(
                tx,
                "DELETE FROM relay_agent_managers WHERE workspace=$1 AND agent=$2",
                vec![w.into(), agent.into()],
            )
            .await?;
            for person in &wanted {
                execute(tx, "INSERT INTO relay_agent_managers(workspace,agent,user_id,added_by,at) VALUES($1,$2,$3,$4,$5)", vec![w.into(), agent.into(), person.clone().into(), actor.into(), now().into()]).await?;
            }
        }
    }
    Ok(changes)
}

/// After `person` leaves, an agent they managed alone is managed by its owner.
pub(super) async fn keep_a_manager(db: &impl ConnectionTrait, w: &str, person: &str) -> Result<()> {
    if !exists(db).await? {
        return Ok(());
    }
    execute(
        db,
        "INSERT INTO relay_agent_managers(workspace,agent,user_id,added_by,at)
         SELECT a.workspace,a.id,a.owner_id,a.owner_id,$3 FROM relay_shared_agents a
         WHERE a.workspace=$1 AND a.owner_id IS NOT NULL AND a.owner_id<>$2
           AND EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.id AND g.user_id=$2)
           AND NOT EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.id AND g.user_id<>$2)
         ON CONFLICT DO NOTHING",
        vec![w.into(), person.into(), now().into()],
    )
    .await?;
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TransferAgent {
    pub owner: String,
    pub revision: i64,
}

impl PgStore {
    /// Hands an agent to another member. The new owner replaces the old one as a
    /// manager; an only-me agent then talks only to its new owner.
    pub(crate) async fn agent_transfer(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &TransferAgent,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        // An exact retry after a lost response succeeds without changing anything,
        // even though the previous owner may no longer manage the agent.
        let retry = query(
            &tx,
            "SELECT 1 AS n FROM relay_shared_agents a WHERE a.workspace=$1 AND a.id=$2 AND a.owner_id=$3 AND a.revision=$4
             AND EXISTS(SELECT 1 FROM relay_audit l WHERE l.workspace=$1 AND l.action='agent_owner_changed' AND l.target=$2 AND l.actor=$5
              AND l.id=(SELECT max(id) FROM relay_audit WHERE workspace=$1 AND action='agent_owner_changed' AND target=$2))",
            vec![w.into(), id.into(), input.owner.clone().into(), (input.revision + 1).into(), user.id.clone().into()],
        )
        .await?;
        if retry.is_some() {
            tx.commit().await?;
            return Ok(json!({"owner":input.owner,"revision":input.revision + 1}));
        }
        let a = super::agent_profiles::managed_access(&tx, &user.id, w, id, true).await?;
        let revision = a["revision"].as_i64().unwrap_or(0);
        let owner = a["owner_id"].as_str().unwrap_or_default().to_string();
        ensure!(
            revision == input.revision,
            "Agent changed; reload before saving"
        );
        ensure!(owner != input.owner, "They already own this agent");
        role(&tx, w, &input.owner)
            .await
            .context("The new owner must be in this workspace")?;
        execute(&tx, "UPDATE relay_shared_agents SET owner_id=$3,revision=revision+1 WHERE workspace=$1 AND id=$2", vec![w.into(), id.into(), input.owner.clone().into()]).await?;
        execute(
            &tx,
            "DELETE FROM relay_agent_managers WHERE workspace=$1 AND agent=$2 AND user_id=$3",
            vec![w.into(), id.into(), owner.into()],
        )
        .await?;
        execute(&tx, "INSERT INTO relay_agent_managers(workspace,agent,user_id,added_by,at) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING", vec![w.into(), id.into(), input.owner.clone().into(), user.id.clone().into(), now().into()]).await?;
        super::task_revocation::inaccessible(&tx, w).await?;
        let actor = super::member_handover::name(&tx, &user.id).await?;
        let heir = super::member_handover::name(&tx, &input.owner).await?;
        let agent = a["name"].as_str().unwrap_or("an agent");
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "agent_owner_changed",
            id,
            &format!("{actor} made {heir} the owner of {agent}"),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"owner":input.owner,"revision":revision + 1}))
    }
}
