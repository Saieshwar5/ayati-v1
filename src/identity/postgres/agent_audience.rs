//! Who can talk to an agent, and its owner (D231 slice 2, D233). Since schema
//! 122 an agent has one owner and no managers; an agent whose owner left has
//! none and stays paused until an admin gives it one.
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
        "SELECT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('relay_shared_agents') AND attname='audience' AND NOT attisdropped) AS present",
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

/// Saves the selected people that `input` names. Reports what changed and
/// whether someone lost access.
pub(super) async fn save_lists(
    tx: &impl ConnectionTrait,
    w: &str,
    input: &SaveAgent,
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
    Ok(changes)
}
