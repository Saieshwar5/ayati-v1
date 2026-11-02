//! Team panels (people-and-agents-first, Phase 3 HP4). The panel's owner shows it to
//! a channel's members or the whole workspace. The audience rule is enforced, not
//! promised: only a panel whose every value is bound to a reviewed source read can
//! be shared; viewers see only snapshots whose values were checked against those
//! reads; the agent's own words (what changed, the proof link) only reach people who
//! can see the conversation they were written in. Updates still run as the owner,
//! and each person arranges (hides, moves) shared panels on their own Home.
use super::super::*;
use crate::identity::home_panels::model::Share;
use crate::panels::bindings::Binding;
use serde_json::json;

#[path = "home_team_migration.rs"]
pub(in crate::identity::postgres) mod migration;
#[cfg(test)]
#[path = "home_team_tests.rs"]
mod tests;

/// Upgrade tests run current code on older schemas.
pub(in crate::identity::postgres) async fn available(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regprocedure('relay_home_team_people(text,text,text)') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .is_some_and(|r| r.try_get::<bool>("", "ready").unwrap_or(false)))
}

/// Every changing value of the section is bound to the source.
fn fully_bound(section: &Value, bindings: &[Binding]) -> std::result::Result<(), String> {
    let has = |t: String| bindings.iter().any(|b| b.target == t);
    let items = section["items"].as_array().map_or(0, Vec::len);
    let ok = match section["kind"].as_str() {
        Some("metric") => has("value".into()) && section["comparison"].is_null(),
        Some("metrics") => {
            (0..items).all(|i| has(format!("items.{i}.value")))
                && section["items"]
                    .as_array()
                    .is_some_and(|v| v.iter().all(|i| i["comparison"].is_null()))
        }
        Some("progress") => (0..items).all(|i| has(format!("items.{i}.actual"))),
        Some("table") => has("rows".into()),
        _ => {
            return Err(
                "Only number, list-of-numbers, progress and table panels can be shared.".into(),
            );
        }
    };
    if ok {
        Ok(())
    } else {
        Err("Every number on it must come straight from the source before it can be shared. Ask for an update; the agent can link each number to its read.".into())
    }
}

/// Why this panel can't be shown to others now, if it can't.
async fn shareable(
    tx: &DatabaseTransaction,
    w: &str,
    p: &Value,
) -> Result<std::result::Result<(), String>> {
    let maintenance = &p["maintenance"];
    if maintenance["enabled"] != true {
        return Ok(Err(
            "Turn on its updates first: a shared panel keeps itself current.".into(),
        ));
    }
    let Ok(bindings) = serde_json::from_value::<Vec<Binding>>(maintenance["bindings"].clone())
    else {
        return Ok(Err("Its numbers must come straight from the source first. Ask for an update; the agent can link each number to its read.".into()));
    };
    if let Err(reason) = fully_bound(&p["definition"]["sections"][0], &bindings) {
        return Ok(Err(reason));
    }
    if latest_checked(
        tx,
        w,
        p["id"].as_str().unwrap_or_default(),
        p["active_revision"].as_i64().unwrap_or(0),
    )
    .await?
    .is_none()
    {
        return Ok(Err(
            "Wait for an update whose numbers were checked against the source.".into(),
        ));
    }
    Ok(Ok(()))
}

/// The audience may see what the panel is about: everyone may use the agent, or the
/// agent is in the channel.
async fn audience_allowed(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    audience: &str,
    chat: Option<&str>,
) -> Result<std::result::Result<(), String>> {
    match (audience, chat) {
        ("workspace", None) => {
            let everyone = query(tx, "SELECT audience='everyone' AS ok FROM relay_shared_agents WHERE workspace=$1 AND id=$2", vec![w.into(), agent.into()]).await?
                .is_some_and(|r| r.try_get::<bool>("", "ok").unwrap_or(false));
            Ok(if everyone {
                Ok(())
            } else {
                Err("Only an agent everyone may use can keep a panel for everyone.".into())
            })
        }
        ("channel", Some(chat)) => {
            let in_channel = query(
                tx,
                "SELECT 1 FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3",
                vec![w.into(), chat.into(), agent.into()],
            )
            .await?
            .is_some();
            Ok(if in_channel {
                Ok(())
            } else {
                Err("The panel's agent must be in that channel.".into())
            })
        }
        _ => Ok(Err("Choose only me, a channel or everyone.".into())),
    }
}

async fn latest_checked(
    db: &impl ConnectionTrait,
    w: &str,
    panel: &str,
    revision: i64,
) -> Result<Option<sea_orm::QueryResult>> {
    query(db, "SELECT s.definition,s.provenance,s.saved_ms,s.checked_ms,s.task,s.generation,s.what_changed,r.direct,m.chat,m.thread
        FROM relay_home_snapshots s JOIN relay_home_update_runs r ON r.workspace=s.workspace AND r.reply=s.task
        JOIN relay_chat_messages m ON m.workspace=s.workspace AND m.id=s.task
        WHERE s.workspace=$1 AND s.panel=$2 AND s.panel_revision=$3 AND r.source_checked ORDER BY s.generation DESC LIMIT 1",
        vec![w.into(), panel.into(), revision.into()]).await
}

/// Was every value of this update checked against its reads? Saving bindings checks
/// them; saved bindings are checked again on every update. A shared panel refuses
/// values that don't match its source.
pub(in crate::identity::postgres) async fn checked(
    tx: &DatabaseTransaction,
    job: &crate::identity::chat::model::Job,
    data: &Value,
    section: &Value,
    kept: bool,
) -> Result<()> {
    if !available(tx).await? {
        return Ok(());
    }
    let ok = kept
        || match serde_json::from_value::<Vec<Binding>>(data["policy"]["bindings"].clone()) {
            Ok(bindings) => {
                let scopes = data["policy"]["scopes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let results =
                    super::super::home_update_runs::living::read_results(tx, job, &scopes).await?;
                match crate::panels::bindings::check(section, &bindings, &results) {
                    Ok(()) => true,
                    Err(error) => {
                        let team = query(tx, "SELECT audience<>'personal' AS team FROM relay_home_panels WHERE workspace=$1 AND id=$2", vec![job.workspace.clone().into(), data["panel"].as_str().unwrap_or_default().into()]).await?
                        .is_some_and(|r| r.try_get::<bool>("", "team").unwrap_or(false));
                        ensure!(
                            !team,
                            "A shared panel shows only numbers read from the source: {error}"
                        );
                        false
                    }
                }
            }
            Err(_) => false,
        };
    execute(
        tx,
        "UPDATE relay_home_update_runs SET source_checked=$3 WHERE workspace=$1 AND reply=$2",
        vec![
            job.workspace.clone().into(),
            job.reply.clone().into(),
            ok.into(),
        ],
    )
    .await?;
    Ok(())
}

/// The owner's view of who sees their panel, and whether it may be shared.
pub(in crate::identity::postgres) async fn decorate(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    panel: &mut Value,
) -> Result<()> {
    if !available(tx).await? || panel["active_revision"].is_null() {
        return Ok(());
    }
    let p = super::super::home_maintenance::owned(
        tx,
        user,
        w,
        panel["id"].as_str().unwrap_or_default(),
    )
    .await?;
    let reason = shareable(tx, w, &p).await?.err();
    let name = match p["audience_chat"].as_str() {
        Some(chat) => query(
            tx,
            "SELECT name FROM relay_chats WHERE workspace=$1 AND id=$2",
            vec![w.into(), chat.into()],
        )
        .await?
        .map(|r| r.try_get::<String>("", "name"))
        .transpose()?,
        None => None,
    };
    panel["audience"] = p["audience"].clone();
    panel["audience_chat"] = p["audience_chat"].clone();
    panel["audience_name"] = json!(name);
    panel["audience_revision"] = p["audience_revision"].clone();
    panel["shareable"] = json!({"ok": reason.is_none(), "reason": reason});
    Ok(())
}

/// Panels others shared with this person, read-only.
pub(in crate::identity::postgres) async fn shared_with(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    pooled: bool,
) -> Result<Vec<Value>> {
    if !available(tx).await? {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for r in rows(tx, "SELECT p.id,p.owner_id,p.agent,p.chat,p.audience,p.audience_chat,p.active_revision,a.name AS agent_name,
          COALESCE(u.display_name,u.email) AS owner_name,c.name AS audience_name
        FROM relay_home_panels p JOIN relay_shared_agents a ON a.workspace=p.workspace AND a.id=p.agent
        JOIN relay_users u ON u.id=p.owner_id LEFT JOIN relay_chats c ON c.workspace=p.workspace AND c.id=p.audience_chat
        WHERE p.workspace=$1 AND p.owner_id<>$2 AND p.audience<>'personal' AND p.active_revision IS NOT NULL
          AND (p.audience='workspace' OR EXISTS(SELECT 1 FROM relay_chat_people x WHERE x.workspace=p.workspace AND x.chat=p.audience_chat AND x.user_id=$2))
        ORDER BY p.created_ms,p.id LIMIT 32", vec![w.into(), user.id.clone().into()]).await? {
        let id: String = r.try_get("", "id")?;
        let owner = User { id: r.try_get("", "owner_id")?, email: String::new(), display_name: None };
        let agent: String = r.try_get("", "agent")?;
        let audience: String = r.try_get("", "audience")?;
        let audience_chat: Option<String> = r.try_get("", "audience_chat")?;
        let revision: i64 = r.try_get("", "active_revision")?;
        // Checked again on every read: the owner's sharing still holds.
        let p = super::super::home_maintenance::owned(tx, &owner, w, &id).await?;
        if shareable(tx, w, &p).await?.is_err() || audience_allowed(tx, w, &agent, &audience, audience_chat.as_deref()).await?.is_err() {
            continue;
        }
        let Some(s) = latest_checked(tx, w, &id, revision).await? else { continue };
        let provenance: Value = s.try_get("", "provenance")?;
        if !super::super::home_sources::allowed(tx, w, &owner.id, &provenance).await? {
            continue;
        }
        let chat: String = r.try_get("", "chat")?;
        let sees_conversation = super::super::chat::access(tx, user, w, &chat).await.is_ok();
        let mut active = json!({"revision": revision, "available": true, "definition": s.try_get::<Value>("", "definition")?,
            "saved_ms": s.try_get::<i64>("", "saved_ms")?, "checked_ms": s.try_get::<i64>("", "checked_ms")?,
            "generation": s.try_get::<i64>("", "generation")?, "from_source": s.try_get::<bool>("", "direct")?,
            "source_operations": provenance["operations"].as_array().map_or(0, Vec::len)});
        if sees_conversation {
            active["what_changed"] = json!(s.try_get::<Option<String>>("", "what_changed")?);
            active["task"] = json!(s.try_get::<String>("", "task")?);
            active["chat"] = json!(s.try_get::<String>("", "chat")?);
            active["thread"] = json!(s.try_get::<Option<String>>("", "thread")?);
        }
        let health = super::super::home_health::maintenance(tx, &owner, w, &id, pooled).await?;
        out.push(json!({"id": id, "shared": true, "revision": revision, "active_revision": revision, "draft": null, "active": active,
            "agent": agent, "agent_name": r.try_get::<String>("", "agent_name")?, "owner_name": r.try_get::<String>("", "owner_name")?,
            "audience": audience, "audience_chat": audience_chat, "audience_name": r.try_get::<Option<String>>("", "audience_name")?,
            "maintenance": {"status": health["status"], "stale_seconds": health["stale_seconds"], "revision": 0, "enabled": true,
                "last_update": if sees_conversation { health["last_update"].clone() } else { Value::Null }}}));
    }
    Ok(out)
}

/// Panel ids this person may place on their Home: their own and those shared with them.
pub(in crate::identity::postgres) const PLACEABLE: &str = "SELECT id FROM relay_home_panels p WHERE p.workspace=$1 AND (p.owner_id=$2 OR (p.audience<>'personal' AND (p.audience='workspace' OR EXISTS(SELECT 1 FROM relay_chat_people x WHERE x.workspace=p.workspace AND x.chat=p.audience_chat AND x.user_id=$2)))) AND p.id IN (SELECT jsonb_array_elements_text($3::jsonb))";

impl PgStore {
    /// The owner chooses who sees their panel. Narrowing is always allowed;
    /// widening needs a panel whose numbers all come from the source.
    pub(crate) async fn share_home_panel(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Share,
    ) -> Result<Value> {
        crate::identity::automations::model::uuid(&input.request)?;
        let tx = self.workspace_tx(w).await?;
        ensure!(
            available(&tx).await?,
            "Team panels need the latest workspace database"
        );
        let p = super::super::home_maintenance::owned(&tx, user, w, id).await?;
        let payload = json!({"audience": id, "input": input});
        if let Some(saved) =
            super::super::home_receipts::retry(&tx, user, w, &input.request, &payload).await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        ensure!(
            p["audience_revision"] == input.revision,
            "Who sees this panel changed on another device; refresh and choose again"
        );
        let chat = if input.audience == "channel" {
            input.chat.as_deref()
        } else {
            None
        };
        if input.audience != "personal" {
            if let Err(reason) = shareable(&tx, w, &p).await? {
                anyhow::bail!(reason);
            }
            if let Err(reason) = audience_allowed(
                &tx,
                w,
                p["agent"].as_str().unwrap_or_default(),
                &input.audience,
                chat,
            )
            .await?
            {
                anyhow::bail!(reason);
            }
            if let Some(chat) = chat {
                ensure!(query(&tx, "SELECT 1 FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3", vec![w.into(), chat.into(), user.id.clone().into()]).await?.is_some(),
                    "You can share only with a channel you're in");
            }
        }
        let row = query(&tx, "UPDATE relay_home_panels SET audience=$3,audience_chat=$4,audience_revision=audience_revision+1 WHERE workspace=$1 AND id=$2 RETURNING audience_revision",
            vec![w.into(), id.into(), input.audience.clone().into(), chat.map(str::to_string).into()]).await?.context("Panel unavailable")?;
        let result = json!({"audience": input.audience, "audience_revision": row.try_get::<i64>("", "audience_revision")?});
        super::super::home_receipts::remember(&tx, user, w, &input.request, &payload, &result)
            .await?;
        audit(&tx, &user.id, Some(w), "home_panel_audience", id).await?;
        tx.commit().await?;
        Ok(result)
    }
}
