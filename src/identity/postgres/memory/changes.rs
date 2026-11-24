//! Changing memory as a person: admins on the workspace's memory, an agent's
//! owners and admins on its job and room notes. Every change is theirs to make
//! on the page's notes only.
use super::manage::{ON_PAGE, Page, allowed};
use super::notes::{NewNote, keep, secret};
use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoteInput {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub topic: String,
    pub text: String,
    /// A date (YYYY-MM-DD) after which it no longer holds, or none.
    #[serde(default)]
    pub until: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TopicInput {
    pub topic: String,
    /// Empty removes the summary.
    pub summary: String,
}

/// The page's note `id`, if the viewer may change it.
async fn on_page(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    page: &Page<'_>,
    id: &str,
) -> Result<Value> {
    json_rows(
        tx,
        &format!("SELECT jsonb_build_object('state',m.state,'replaces',m.replaces) AS data FROM relay_memories m
          LEFT JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
          WHERE m.workspace=$1 AND {ON_PAGE} AND m.id=$4"),
        vec![w.into(), user.id.clone().into(), page.agent().into(), id.into()],
    )
    .await?
    .into_iter()
    .next()
    .context("That note is not on this page")
}

impl PgStore {
    /// Adds a note to the page's shelf, or changes the wording of one on it.
    /// A person's note is confirmed: it starts strong.
    pub(crate) async fn save_memory_note(
        &self,
        user: &User,
        w: &str,
        page: Page<'_>,
        input: &NoteInput,
    ) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&input.id).is_ok(),
            "Invalid note identity"
        );
        ensure!(
            KINDS.contains(&input.kind.as_str()),
            "Choose what the note is about"
        );
        let text = line(&input.text, 300);
        ensure!(!text.is_empty(), "Write the note");
        ensure!(
            input.text.trim().chars().count() <= 300,
            "Keep it to one line, up to 300 characters"
        );
        ensure!(
            !secret(&text),
            "Passwords, keys, codes and card numbers are never kept in memory"
        );
        let topic = line(&input.topic, 80);
        let until = match input.until.as_deref().filter(|d| !d.trim().is_empty()) {
            Some(date) => Some(
                super::notes::until_ms(date, now() * 1000)
                    .context("Choose a date ahead, within two years")?,
            ),
            None => None,
        };
        let tx = self.workspace_tx(w).await?;
        allowed(&tx, user, w, &page).await?;
        ensure!(ready(&tx).await?, "Memory is not available yet");
        let exists = on_page(&tx, user, w, &page, &input.id).await.is_ok();
        if exists {
            execute(
                &tx,
                &format!("UPDATE relay_memories SET kind=$3,topic=$4,text=$5,until_ms=$6,source='admin',author=$7,strength=greatest(strength,{CONFIRMED_STRENGTH}),changed_ms={NOW_MS} WHERE workspace=$1 AND id=$2"),
                vec![w.into(), input.id.clone().into(), input.kind.clone().into(), topic.into(), text.into(), until.into(), user.id.clone().into()],
            )
            .await?;
        } else {
            let shelf = match page {
                Page::Workspace => Shelf::workspace(),
                Page::Agent(agent) => Shelf::job(agent),
            };
            let kept = keep(
                &tx,
                w,
                NewNote {
                    id: &input.id,
                    shelf: &shelf,
                    kind: &input.kind,
                    topic: &topic,
                    text: &text,
                    source: "admin",
                    author: Some(&user.id),
                    job: None,
                    learned_in: None,
                    strength: CONFIRMED_STRENGTH,
                    waiting: None,
                    until_ms: until,
                    replaces: None,
                },
            )
            .await?;
            ensure!(kept != "full", "That memory is full. Remove notes first.");
            ensure!(
                kept != "waiting_known",
                "The same note already waits for an answer above"
            );
        }
        tx.commit().await?;
        Ok(())
    }

    /// One change to a note on the page: pin, unpin, confirm, essential,
    /// unessential, end, restore, remove, keep (a waiting note) or discard.
    pub(crate) async fn change_memory_note(
        &self,
        user: &User,
        w: &str,
        page: Page<'_>,
        id: &str,
        change: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        allowed(&tx, user, w, &page).await?;
        ensure!(ready(&tx).await?, "Memory is not available yet");
        let found = on_page(&tx, user, w, &page, id).await?;
        let state = found["state"].as_str().unwrap_or_default();
        let set = match change {
            "pin" => "pinned=true",
            "unpin" => "pinned=false",
            "confirm" => {
                "strength=greatest(strength,90),used_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint"
            }
            "essential" => "essential=true",
            "unessential" => "essential=false",
            "end" => "state='ended'",
            // Brought back as if just used, so it does not fade away at once.
            "restore" => {
                "state='active',used_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,strength=greatest(strength,50)"
            }
            "keep" => {
                "state='active',waiting_reason=NULL,strength=greatest(strength,90),used_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint"
            }
            "remove" | "discard" => "",
            _ => anyhow::bail!("Unknown memory change"),
        };
        match change {
            "keep" | "discard" => {
                ensure!(state == "waiting", "That note is not waiting for an answer")
            }
            "restore" => ensure!(
                ["ended", "archived", "folded"].contains(&state),
                "That note is already in use"
            ),
            "remove" => {}
            _ => ensure!(state == "active", "That note is not in use"),
        }
        if set.is_empty() {
            execute(
                &tx,
                "DELETE FROM relay_memories WHERE workspace=$1 AND id=$2",
                vec![w.into(), id.into()],
            )
            .await?;
        } else {
            execute(
                &tx,
                &format!("UPDATE relay_memories SET {set},changed_ms={NOW_MS} WHERE workspace=$1 AND id=$2"),
                vec![w.into(), id.into()],
            )
            .await?;
        }
        // Keeping a correction ends the note it corrects.
        if change == "keep"
            && let Some(old) = found["replaces"].as_str()
        {
            execute(
                &tx,
                &format!("UPDATE relay_memories SET state='ended',replaced_by=$3,changed_ms={NOW_MS} WHERE workspace=$1 AND id=$2 AND state='active'"),
                vec![w.into(), old.into(), id.into()],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Writes or removes the summary of one topic on the page's shelf.
    pub(crate) async fn save_memory_topic(
        &self,
        user: &User,
        w: &str,
        page: Page<'_>,
        input: &TopicInput,
    ) -> Result<()> {
        let topic = line(&input.topic, 80);
        ensure!(!topic.is_empty(), "Name the topic");
        let summary = input.summary.trim();
        ensure!(
            summary.chars().count() <= 1200,
            "Keep the summary under 1,200 characters"
        );
        ensure!(
            !secret(summary),
            "Passwords, keys, codes and card numbers are never kept in memory"
        );
        let tx = self.workspace_tx(w).await?;
        allowed(&tx, user, w, &page).await?;
        ensure!(ready(&tx).await?, "Memory is not available yet");
        let shelf = match page {
            Page::Workspace => Shelf::workspace(),
            Page::Agent(agent) => Shelf::job(agent),
        };
        if summary.is_empty() {
            execute(
                &tx,
                "DELETE FROM relay_memory_topics WHERE workspace=$1 AND shelf=$2 AND topic=$3",
                vec![w.into(), shelf.key().into(), topic.into()],
            )
            .await?;
        } else {
            execute(
                &tx,
                &format!("INSERT INTO relay_memory_topics(workspace,scope,agent,topic,summary,source) VALUES($1,$2,$3,$4,$5,'admin')
                 ON CONFLICT(workspace,shelf,topic) DO UPDATE SET summary=EXCLUDED.summary,source='admin',updated_ms={NOW_MS}"),
                vec![w.into(), shelf.scope.into(), shelf.agent.clone().into(), topic.into(), summary.into()],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Undoes one change the tidy-up made: the notes and summary come back as they were.
    pub(crate) async fn undo_memory_change(
        &self,
        user: &User,
        w: &str,
        page: Page<'_>,
        id: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        allowed(&tx, user, w, &page).await?;
        ensure!(ready(&tx).await?, "Memory is not available yet");
        let row = query(
            &tx,
            "SELECT before,undone_ms FROM relay_memory_changes WHERE workspace=$1 AND id=$2
               AND (($3='' AND scope='workspace') OR ($3<>'' AND agent=$3 AND scope='job')) FOR UPDATE",
            vec![w.into(), id.into(), page.agent().into()],
        )
        .await?
        .context("That change is not on this page")?;
        ensure!(
            row.try_get::<Option<i64>>("", "undone_ms")?.is_none(),
            "That change was already undone"
        );
        let before: Value = row.try_get("", "before")?;
        restore(&tx, &before).await?;
        execute(
            &tx,
            &format!(
                "UPDATE relay_memory_changes SET undone_ms={NOW_MS} WHERE workspace=$1 AND id=$2"
            ),
            vec![w.into(), id.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Puts back notes and a topic summary as a change recorded them.
pub(crate) async fn restore(tx: &DatabaseTransaction, before: &Value) -> Result<()> {
    for memory in before["memories"].as_array().into_iter().flatten() {
        execute(
            tx,
            "INSERT INTO relay_memories(workspace,id,scope,agent,chat,user_id,kind,topic,text,source,author,job,learned_in,state,waiting_reason,
               strength,pinned,essential,used,used_ms,until_ms,replaces,replaced_by,created_ms,changed_ms)
             SELECT r.workspace,r.id,r.scope,r.agent,r.chat,r.user_id,r.kind,r.topic,r.text,r.source,r.author,r.job,r.learned_in,r.state,r.waiting_reason,
               r.strength,r.pinned,r.essential,r.used,r.used_ms,r.until_ms,r.replaces,r.replaced_by,r.created_ms,r.changed_ms
             FROM jsonb_populate_record(NULL::relay_memories,$1) r
             ON CONFLICT(workspace,id) DO UPDATE SET state=EXCLUDED.state,waiting_reason=EXCLUDED.waiting_reason,strength=EXCLUDED.strength,
               used=EXCLUDED.used,used_ms=EXCLUDED.used_ms,replaced_by=EXCLUDED.replaced_by,text=EXCLUDED.text,changed_ms=EXCLUDED.changed_ms",
            vec![memory.clone().into()],
        )
        .await?;
    }
    if let Some(topic) = before.get("topic") {
        let (w, shelf, name) = (
            &before["workspace"],
            &before["shelf"],
            &before["topic_name"],
        );
        if topic.is_null() {
            execute(
                tx,
                "DELETE FROM relay_memory_topics WHERE workspace=$1 AND shelf=$2 AND topic=$3",
                vec![
                    w.as_str().unwrap_or_default().into(),
                    shelf.as_str().unwrap_or_default().into(),
                    name.as_str().unwrap_or_default().into(),
                ],
            )
            .await?;
        } else {
            execute(
                tx,
                "INSERT INTO relay_memory_topics(workspace,scope,agent,chat,user_id,topic,summary,source,updated_ms)
                 SELECT r.workspace,r.scope,r.agent,r.chat,r.user_id,r.topic,r.summary,r.source,r.updated_ms FROM jsonb_populate_record(NULL::relay_memory_topics,$1) r
                 ON CONFLICT(workspace,shelf,topic) DO UPDATE SET summary=EXCLUDED.summary,source=EXCLUDED.source,updated_ms=EXCLUDED.updated_ms",
                vec![topic.clone().into()],
            )
            .await?;
        }
    }
    Ok(())
}
