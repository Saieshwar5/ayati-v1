//! An agent's owners (schema 122): one or more equal people. Every owner
//! changes the agent, decides its "Ask first" actions, sees all its work,
//! pauses it and adds or removes owners, without asking the others. Admins
//! have the same authority over every agent. An agent keeps at least one owner
//! while people choose; it loses its last one only when that person leaves the
//! workspace, and then pauses until an admin gives it one.
use super::*;
use serde_json::json;

/// The most owners one agent has.
const MOST: i64 = 10;

/// Whether schema 122 is applied. Upgrade tests run this code on older schemas.
pub(super) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_agent_owners') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// Whether `person` is one of `agent`'s owners. Before schema 122 an agent's
/// single `owner_id` is its owner.
pub(super) async fn owns(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
    person: &str,
) -> Result<bool> {
    let sql = if ready(db).await? {
        "SELECT relay_agent_owns($1,$2,$3) AS owns"
    } else {
        "SELECT EXISTS(SELECT 1 FROM relay_shared_agents WHERE workspace=$1 AND id=$2 AND owner_id=$3) AS owns"
    };
    Ok(query(db, sql, vec![w.into(), agent.into(), person.into()])
        .await?
        .context("Agent unavailable")?
        .try_get("", "owns")?)
}

/// The owners, first added first: `{id, name}`.
pub(super) async fn listed(db: &impl ConnectionTrait, w: &str, agent: &str) -> Result<Vec<Value>> {
    json_rows(
        db,
        "SELECT jsonb_build_object('id',u.id,'name',COALESCE(u.display_name,u.email)) AS data
        FROM relay_agent_owners o JOIN relay_users u ON u.id=o.user_id
        WHERE o.workspace=$1 AND o.agent=$2 ORDER BY o.at,o.user_id",
        vec![w.into(), agent.into()],
    )
    .await
}

/// `person` is leaving the workspace. Agents with other owners stay with them.
/// Agents they owned alone go to `new_owner` (an only-me one paused, for the new
/// owner to look at), or are left without an owner, paused, remembering whether
/// they were running so they resume when they get one.
pub(super) async fn owner_leaves(
    tx: &impl ConnectionTrait,
    w: &str,
    person: &str,
    new_owner: Option<&str>,
    heir: &str,
) -> Result<()> {
    if !ready(tx).await? {
        // Before schema 122 an agent always had an owner: the person taking over.
        execute(tx, "UPDATE relay_shared_agents SET owner_id=$3,revision=revision+1,settings=CASE WHEN audience='only_me' THEN jsonb_set(settings,'{paused}','true') ELSE settings END WHERE workspace=$1 AND owner_id=$2", vec![w.into(), person.into(), heir.into()]).await?;
        return Ok(());
    }
    // The agents they own alone, read before anything changes.
    let alone: Vec<String> = rows(tx, "SELECT a.id FROM relay_shared_agents a WHERE a.workspace=$1
        AND EXISTS(SELECT 1 FROM relay_agent_owners o WHERE o.workspace=a.workspace AND o.agent=a.id AND o.user_id=$2)
        AND NOT EXISTS(SELECT 1 FROM relay_agent_owners o WHERE o.workspace=a.workspace AND o.agent=a.id AND o.user_id<>$2)",
        vec![w.into(), person.into()]).await?
        .iter()
        .map(|row| row.try_get("", "id"))
        .collect::<std::result::Result<_, _>>()?;
    for agent in alone {
        match new_owner {
            Some(owner) => {
                execute(tx, "INSERT INTO relay_agent_owners(workspace,agent,user_id,added_by) VALUES($1,$2,$3,$3) ON CONFLICT DO NOTHING",
                    vec![w.into(), agent.clone().into(), owner.into()]).await?;
                execute(tx, "UPDATE relay_shared_agents SET revision=revision+1,settings=CASE WHEN audience='only_me' THEN jsonb_set(settings,'{paused}','true') ELSE settings END
                    WHERE workspace=$1 AND id=$2", vec![w.into(), agent.into()]).await?;
            }
            None => {
                execute(tx, "UPDATE relay_shared_agents SET revision=revision+1,
                    resume_with_owner=(audience<>'only_me' AND COALESCE(settings->>'paused','false')<>'true'),
                    settings=jsonb_set(settings,'{paused}','true') WHERE workspace=$1 AND id=$2", vec![w.into(), agent.into()]).await?;
            }
        }
    }
    execute(
        tx,
        "DELETE FROM relay_agent_owners WHERE workspace=$1 AND user_id=$2",
        vec![w.into(), person.into()],
    )
    .await?;
    Ok(())
}

/// Adds or removes one owner, as any owner or an admin may.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnersChange {
    #[serde(default)]
    pub add: Option<String>,
    #[serde(default)]
    pub remove: Option<String>,
    pub revision: i64,
}

impl PgStore {
    /// The agent's owners, for anyone who may see the agent.
    pub(crate) async fn agent_owners(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let a = super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let owners = listed(&tx, w, agent).await?;
        let can_change =
            owns(&tx, w, agent, &user.id).await? || role(&tx, w, &user.id).await? == "Admin";
        tx.commit().await?;
        Ok(json!({"owners":owners,"can_change":can_change,"revision":a["revision"]}))
    }

    /// Adds or removes one owner. Repeating a change already made changes
    /// nothing, so a retried request is harmless. The last owner can't be
    /// removed; an agent without owners gets one and, if it was running when
    /// its last owner left, resumes.
    pub(crate) async fn change_agent_owners(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        change: &OwnersChange,
    ) -> Result<Value> {
        ensure!(
            change.add.is_some() != change.remove.is_some(),
            "Add or remove one owner at a time"
        );
        let tx = self.workspace_tx(w).await?;
        let a = super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        ensure!(
            ready(&tx).await?,
            "Run workspace-db migrate to change owners"
        );
        let name = a["name"].as_str().unwrap_or("an agent").to_owned();
        let actor = super::member_handover::name(&tx, &user.id).await?;
        let mut done = None;
        if let Some(person) = &change.add {
            role(&tx, w, person)
                .await
                .context("Owners must be in this workspace")?;
            if !owns(&tx, w, agent, person).await? {
                ensure!(
                    a["revision"].as_i64() == Some(change.revision),
                    "Agent changed; reload before saving"
                );
                let count = listed(&tx, w, agent).await?.len() as i64;
                ensure!(count < MOST, "An agent can have up to {MOST} owners");
                execute(&tx, "INSERT INTO relay_agent_owners(workspace,agent,user_id,added_by) VALUES($1,$2,$3,$4)",
                    vec![w.into(), agent.into(), person.clone().into(), user.id.clone().into()]).await?;
                // An agent that had no owner and was running when its owner left resumes.
                execute(&tx, "UPDATE relay_shared_agents SET revision=revision+1,
                    settings=CASE WHEN resume_with_owner THEN jsonb_set(settings,'{paused}','false') ELSE settings END,resume_with_owner=false
                    WHERE workspace=$1 AND id=$2", vec![w.into(), agent.into()]).await?;
                let who = super::member_handover::name(&tx, person).await?;
                done = Some((
                    "agent_owner_added",
                    format!("{actor} made {who} an owner of {name}"),
                ));
            }
        }
        if let Some(person) = &change.remove
            && owns(&tx, w, agent, person).await?
        {
            ensure!(
                a["revision"].as_i64() == Some(change.revision),
                "Agent changed; reload before saving"
            );
            ensure!(
                listed(&tx, w, agent).await?.len() > 1,
                "{name} needs at least one owner. Add another owner before removing this one."
            );
            execute(
                &tx,
                "DELETE FROM relay_agent_owners WHERE workspace=$1 AND agent=$2 AND user_id=$3",
                vec![w.into(), agent.into(), person.clone().into()],
            )
            .await?;
            execute(
                &tx,
                "UPDATE relay_shared_agents SET revision=revision+1 WHERE workspace=$1 AND id=$2",
                vec![w.into(), agent.into()],
            )
            .await?;
            // Work for an only-me agent that they may no longer ask for stops.
            super::task_revocation::inaccessible(&tx, w).await?;
            let who = super::member_handover::name(&tx, person).await?;
            done = Some((
                "agent_owner_removed",
                if *person == user.id {
                    format!("{actor} stopped being an owner of {name}")
                } else {
                    format!("{actor} removed {who} as an owner of {name}")
                },
            ));
        }
        if let Some((action, summary)) = done {
            audit_with_summary(&tx, &user.id, Some(w), action, agent, &summary).await?;
        }
        let owners = listed(&tx, w, agent).await?;
        let revision: i64 = query(
            &tx,
            "SELECT revision FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
            vec![w.into(), agent.into()],
        )
        .await?
        .context("Agent unavailable")?
        .try_get("", "revision")?;
        tx.commit().await?;
        Ok(json!({"owners":owners,"revision":revision}))
    }
}

pub(crate) use migration::AgentOwners;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct AgentOwners;
    impl MigrationName for AgentOwners {
        fn name(&self) -> &str {
            "m20261115_000122_agent_owners"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for AgentOwners {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("agent_owners.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Agent owners require a forward migration".into(),
            ))
        }
    }
}
