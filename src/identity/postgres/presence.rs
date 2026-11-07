//! Who has Cuckoo open, and the status people set (schema 98, D245).
//!
//! Presence comes from the live stream, never from a browser timer: while
//! someone's stream is open, the controller holding it records a beat for them
//! (`beat_presence`). A beat within `MIN_GAP_MS` of the last one, from another
//! tab or another controller, changes nothing, so an online person costs about
//! one row update every 30 seconds however many tabs they have. Someone is
//! online while their last beat is under `TTL_MS` old, worked out when read.
//!
//! Only changes people can see are announced: coming online, going offline,
//! a status, and hiding or showing presence. Each raises the row's revision and
//! sends the new view on `relay_presence` in the transaction that makes it;
//! nothing goes into the change journal. Going offline is noticed by a sweep
//! (`expire_presence`) that one controller at a time runs when the earliest
//! online person could expire; the row lock makes each such change happen once.
//!
//! Members read only their own workspace's rows. Someone who turned off "Show
//! when I'm online" is never shown online or last seen, even to themselves, and
//! their own beats announce nothing; their status still shows. Agents are not
//! members, so they never have presence. A status's end is checked when read,
//! so nothing has to clear it.
use super::*;
use crate::identity::chat::status::Status;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;

#[cfg(test)]
#[path = "presence_tests.rs"]
mod tests;

/// The PostgreSQL channel; each payload is `<schema>/<json change>`.
pub(crate) const CHANNEL: &str = "relay_presence";
/// Online means a beat this recently: about the last minute, with room for one
/// beat that arrives late.
pub(crate) const TTL_MS: i64 = 70_000;
/// Beats nearer the last one than this write nothing.
pub(crate) const MIN_GAP_MS: i64 = 28_000;
/// People one sweep takes offline; any more go in the next one.
const SWEEP_LIMIT: i64 = 500;

/// Unix milliseconds by this controller's clock.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as i64)
}

pub(crate) struct MemberPresence;
impl MigrationName for MemberPresence {
    fn name(&self) -> &str {
        "m20261107_000098_member_presence"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for MemberPresence {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("presence.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Presence requires a forward migration".into(),
        ))
    }
}

/// One person's presence as a live stream announces it to their workspace.
#[derive(Debug, PartialEq)]
pub(crate) struct Change {
    pub workspace: String,
    /// The view of [`view`]: id, online, last_seen, status and revision.
    pub person: Value,
}
impl Change {
    /// A notification for `schema`; one for another schema, or unreadable, is ignored.
    pub(crate) fn parse(schema: &str, payload: &str) -> Option<Self> {
        let change: Value = serde_json::from_str(
            payload
                .strip_prefix(schema)
                .and_then(|rest| rest.strip_prefix('/'))?,
        )
        .ok()?;
        Some(Self {
            workspace: change["workspace"].as_str()?.to_owned(),
            person: change
                .get("person")
                .filter(|person| person.is_object())?
                .clone(),
        })
    }
}

const COLUMNS: &str = "p.workspace,p.user_id,p.seen_ms,p.online,p.visible,p.status_kind,p.status_emoji,p.status_text,p.status_clear_ms,p.status_set_ms,p.revision";

/// What members see of one person. Never the raw beats, and never whether
/// they chose to hide them: hidden reads like someone never seen.
fn view(row: &QueryResult, now_ms: i64) -> Result<Value> {
    let visible: bool = row.try_get("", "visible")?;
    let seen: Option<i64> = row.try_get("", "seen_ms")?;
    let online = visible
        && row.try_get::<bool>("", "online")?
        && seen.is_some_and(|seen| seen > now_ms - TTL_MS);
    let kind: Option<String> = row.try_get("", "status_kind")?;
    let clear: Option<i64> = row.try_get("", "status_clear_ms")?;
    let status = match kind {
        Some(kind) if clear.is_none_or(|clear| clear > now_ms) => json!({
            "kind": kind,
            "emoji": row.try_get::<Option<String>>("", "status_emoji")?,
            "text": row.try_get::<Option<String>>("", "status_text")?,
            "clear_at": clear,
            "set_at": row.try_get::<Option<i64>>("", "status_set_ms")?,
        }),
        _ => Value::Null,
    };
    Ok(json!({
        "id": row.try_get::<String>("", "user_id")?,
        "online": online,
        "last_seen": if visible { seen } else { None },
        "status": status,
        "revision": row.try_get::<i64>("", "revision")?,
    }))
}

/// Everyone in the workspace who has presence or a status. The caller has
/// checked that the reader is a member.
pub(super) async fn workspace_views(
    db: &impl ConnectionTrait,
    w: &str,
    now_ms: i64,
) -> Result<Vec<Value>> {
    rows(
        db,
        &format!("SELECT {COLUMNS} FROM relay_member_presence p WHERE p.workspace=$1"),
        vec![w.into()],
    )
    .await?
    .iter()
    .map(|row| view(row, now_ms))
    .collect()
}

/// Sends these people's views to their workspaces' live streams at commit.
async fn announce(
    tx: &DatabaseTransaction,
    people: &[(String, String)],
    now_ms: i64,
) -> Result<()> {
    if people.is_empty() {
        return Ok(());
    }
    let (workspaces, users): (Vec<String>, Vec<String>) = people.iter().cloned().unzip();
    let changed = rows(
        tx,
        &format!(
            "SELECT {COLUMNS} FROM relay_member_presence p
        JOIN unnest($1::text[],$2::text[]) AS t(w,u) ON p.workspace=t.w AND p.user_id=t.u"
        ),
        vec![workspaces.into(), users.into()],
    )
    .await?;
    let mut payloads = Vec::with_capacity(changed.len());
    for row in &changed {
        payloads.push(json!({"workspace": row.try_get::<String>("", "workspace")?, "person": view(row, now_ms)?}).to_string());
    }
    rows(tx, "SELECT pg_notify($1, current_schema() || '/' || p)::text AS sent FROM unnest($2::text[]) AS p",
        vec![CHANNEL.into(), payloads.into()]).await?;
    Ok(())
}

/// People whose view changed, from rows returning `workspace,user_id,visible`;
/// hidden people's beats change nothing anyone sees.
fn shown(changed: &[QueryResult]) -> Result<Vec<(String, String)>> {
    let mut shown = vec![];
    for row in changed {
        if row.try_get::<bool>("", "visible")? {
            shown.push((row.try_get("", "workspace")?, row.try_get("", "user_id")?));
        }
    }
    Ok(shown)
}

async fn sweep_claimed(tx: &DatabaseTransaction) -> Result<bool> {
    Ok(query(tx, "SELECT pg_try_advisory_xact_lock(hashtextextended(current_database() || '/' || current_schema() || '/relay_presence_sweep',0)) AS locked", vec![])
        .await?
        .context("Presence lock unavailable")?
        .try_get("", "locked")?)
}

impl PgStore {
    /// A beat for each (workspace, person) with a live stream open on this
    /// controller. Returns how many came online. Former members are skipped.
    pub(crate) async fn beat_presence(
        &self,
        people: &[(String, String)],
        now_ms: i64,
    ) -> Result<usize> {
        if people.is_empty() {
            return Ok(0);
        }
        let (workspaces, users): (Vec<String>, Vec<String>) = people.iter().cloned().unzip();
        let tx = self.db.begin().await?;
        // Three kinds of beat: a refresh for someone online (only once the last is
        // MIN_GAP_MS old), coming back online, and someone's first.
        let came = rows(&tx, "WITH want AS (
              SELECT DISTINCT m.workspace,m.user_id FROM unnest($1::text[],$2::text[]) AS t(w,u)
              JOIN relay_members m ON m.workspace=t.w AND m.user_id=t.u
            ), kept AS (
              UPDATE relay_member_presence p SET seen_ms=$3 FROM want
              WHERE p.workspace=want.workspace AND p.user_id=want.user_id
              AND p.online AND p.seen_ms>$3-$4 AND p.seen_ms<=$3-$5 RETURNING p.user_id
            ), back AS (
              UPDATE relay_member_presence p SET seen_ms=$3,online=TRUE,revision=p.revision+CASE WHEN p.visible THEN 1 ELSE 0 END
              FROM want WHERE p.workspace=want.workspace AND p.user_id=want.user_id
              AND NOT (p.online AND p.seen_ms>$3-$4) RETURNING p.workspace,p.user_id,p.visible
            ), first AS (
              INSERT INTO relay_member_presence(workspace,user_id,seen_ms,online,revision)
              SELECT workspace,user_id,$3,TRUE,1 FROM want WHERE NOT EXISTS(
                SELECT 1 FROM relay_member_presence p WHERE p.workspace=want.workspace AND p.user_id=want.user_id)
              ON CONFLICT DO NOTHING RETURNING workspace,user_id,visible
            ) SELECT workspace,user_id,visible FROM back UNION ALL SELECT workspace,user_id,visible FROM first",
            vec![workspaces.into(), users.into(), now_ms.into(), TTL_MS.into(), MIN_GAP_MS.into()]).await?;
        if !came.is_empty() {
            // The sweep can now take them offline, so it wakes (work_signals.rs).
            query(
                &tx,
                "SELECT pg_notify($1, current_schema() || '/presence')::text AS sent",
                vec![super::work_signals::CHANNEL.into()],
            )
            .await?;
        }
        announce(&tx, &shown(&came)?, now_ms).await?;
        tx.commit().await?;
        Ok(came.len())
    }

    /// Takes offline everyone whose last beat expired, and returns how long
    /// until the next online person could; None when nobody is online. Another
    /// controller sweeping now means this one looks again a little later.
    pub(crate) async fn expire_presence(&self, now_ms: i64) -> Result<Option<std::time::Duration>> {
        let tx = self.db.begin().await?;
        if !sweep_claimed(&tx).await? {
            return Ok(Some(std::time::Duration::from_millis(MIN_GAP_MS as u64)));
        }
        let gone = rows(&tx, "WITH due AS (
              SELECT workspace,user_id FROM relay_member_presence WHERE online AND seen_ms<=$1-$2
              ORDER BY seen_ms LIMIT $3
            ) UPDATE relay_member_presence p SET online=FALSE,revision=p.revision+CASE WHEN p.visible THEN 1 ELSE 0 END
            FROM due WHERE p.workspace=due.workspace AND p.user_id=due.user_id AND p.online AND p.seen_ms<=$1-$2
            RETURNING p.workspace,p.user_id,p.visible",
            vec![now_ms.into(), TTL_MS.into(), SWEEP_LIMIT.into()]).await?;
        announce(&tx, &shown(&gone)?, now_ms).await?;
        let next: Option<i64> = query(
            &tx,
            "SELECT min(seen_ms) AS next FROM relay_member_presence WHERE online",
            vec![],
        )
        .await?
        .context("Presence unavailable")?
        .try_get("", "next")?;
        tx.commit().await?;
        Ok(next
            .map(|seen| std::time::Duration::from_millis((seen + TTL_MS - now_ms).max(0) as u64)))
    }

    /// Everyone's presence and status, for a member of the workspace.
    pub(crate) async fn presence(&self, user: &User, w: &str, now_ms: i64) -> Result<Vec<Value>> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let people = workspace_views(&tx, w, now_ms).await?;
        tx.commit().await?;
        Ok(people)
    }

    /// Sets the person's own status, or clears it with None; returns their view.
    /// `status` has been through [`Status::checked`].
    pub(crate) async fn set_status(
        &self,
        user: &User,
        w: &str,
        status: Option<&Status>,
        now_ms: i64,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let me = (w.to_owned(), user.id.clone());
        let changed = match status {
            Some(status) => execute(&tx, "INSERT INTO relay_member_presence AS p(workspace,user_id,status_kind,status_emoji,status_text,status_clear_ms,status_set_ms,revision)
                VALUES($1,$2,$3,$4,$5,$6,$7,1) ON CONFLICT(workspace,user_id) DO UPDATE SET status_kind=EXCLUDED.status_kind,
                status_emoji=EXCLUDED.status_emoji,status_text=EXCLUDED.status_text,status_clear_ms=EXCLUDED.status_clear_ms,
                status_set_ms=EXCLUDED.status_set_ms,revision=p.revision+1",
                vec![me.0.clone().into(), me.1.clone().into(), status.kind.clone().into(), status.emoji.clone().into(),
                    status.text.clone().into(), status.clear_at.into(), now_ms.into()]).await?,
            None => execute(&tx, "UPDATE relay_member_presence SET status_kind=NULL,status_emoji=NULL,status_text=NULL,
                status_clear_ms=NULL,status_set_ms=NULL,revision=revision+1 WHERE workspace=$1 AND user_id=$2 AND status_kind IS NOT NULL",
                vec![me.0.clone().into(), me.1.clone().into()]).await?,
        };
        if changed > 0 {
            announce(&tx, std::slice::from_ref(&me), now_ms).await?;
        }
        let own = query(&tx, &format!("SELECT {COLUMNS} FROM relay_member_presence p WHERE p.workspace=$1 AND p.user_id=$2"),
            vec![me.0.into(), me.1.clone().into()]).await?;
        let view = match own {
            Some(row) => view(&row, now_ms)?,
            None => {
                json!({"id": me.1, "online": false, "last_seen": null, "status": null, "revision": 0})
            }
        };
        tx.commit().await?;
        Ok(view)
    }
}

/// "Show when I'm online"; on unless the person turned it off.
pub(in crate::identity::postgres) async fn visible(
    db: &impl ConnectionTrait,
    w: &str,
    user: &str,
) -> Result<bool> {
    Ok(query(
        db,
        "SELECT visible FROM relay_member_presence WHERE workspace=$1 AND user_id=$2",
        vec![w.into(), user.into()],
    )
    .await?
    .map(|row| row.try_get::<bool>("", "visible"))
    .transpose()?
    .unwrap_or(true))
}

/// Turns "Show when I'm online" on or off inside the caller's workspace
/// transaction; everyone's open tabs follow at commit.
pub(in crate::identity::postgres) async fn set_visible(
    tx: &DatabaseTransaction,
    w: &str,
    user: &str,
    visible: bool,
) -> Result<()> {
    let changed: i64 = query(tx, "WITH changed AS (
          UPDATE relay_member_presence SET visible=$3,revision=revision+1
          WHERE workspace=$1 AND user_id=$2 AND visible<>$3 RETURNING 1
        ), added AS (
          INSERT INTO relay_member_presence(workspace,user_id,visible,revision) SELECT $1,$2,$3,1
          WHERE NOT $3 AND NOT EXISTS(SELECT 1 FROM relay_member_presence WHERE workspace=$1 AND user_id=$2)
          ON CONFLICT DO NOTHING RETURNING 1
        ) SELECT (SELECT count(*) FROM changed)+(SELECT count(*) FROM added) AS changed",
        vec![w.into(), user.into(), visible.into()])
        .await?
        .context("Presence unavailable")?
        .try_get("", "changed")?;
    if changed > 0 {
        announce(tx, &[(w.to_owned(), user.to_owned())], now_ms()).await?;
    }
    Ok(())
}
