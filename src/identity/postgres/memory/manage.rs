//! Reading memory as a person: the workspace's memory for admins (Settings ›
//! Memory), an agent's job and room notes for its owners and admins (its page),
//! and what an agent remembers about you, for you alone.
use super::super::agent_profiles::managed_access;
use super::*;
use serde_json::json;

/// Whose memory a page shows: the workspace's, or one agent's.
pub(crate) enum Page<'a> {
    Workspace,
    Agent(&'a str),
}

impl Page<'_> {
    pub(crate) fn agent(&self) -> &str {
        match self {
            Page::Workspace => "",
            Page::Agent(agent) => agent,
        }
    }
}

/// Only admins open the workspace's memory; an agent's owners and admins open its own.
pub(crate) async fn allowed(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    page: &Page<'_>,
) -> Result<()> {
    match page {
        Page::Workspace => {
            manager(db, w, &user.id)
                .await
                .map_err(|_| anyhow::anyhow!("Only admins can open the workspace's memory"))?;
        }
        Page::Agent(agent) => {
            managed_access(db, &user.id, w, agent, true).await?;
        }
    }
    Ok(())
}

/// The notes a page shows, as a condition on `m` (with `c`, its room): the
/// workspace's own, or the agent's job notes and its notes in rooms the
/// viewer ($2) can open. Never a person's notes.
pub(crate) const ON_PAGE: &str = "(($3='' AND m.scope='workspace') OR ($3<>'' AND m.agent=$3 AND (m.scope='job'
    OR (m.scope='room' AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=m.workspace AND p.chat=m.chat AND p.user_id=$2))))))";
const FROM: &str = "FROM relay_memories m
    LEFT JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
    LEFT JOIN relay_chats l ON l.workspace=m.workspace AND l.id=m.learned_in
    LEFT JOIN relay_shared_agents a ON a.workspace=m.workspace AND a.id=m.author
    LEFT JOIN relay_users u ON u.id=m.author";
/// One note as a page shows it. Where it was learned is named only when the
/// viewer can open that conversation.
fn note() -> String {
    format!("jsonb_build_object('id',m.id,'scope',m.scope,'kind',m.kind,'topic',m.topic,'text',m.text,'source',m.source,
    'by',COALESCE(a.name,u.display_name),'room',CASE WHEN m.scope='room' THEN c.name END,
    'learned_in',CASE WHEN l.id IS NOT NULL AND (l.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=l.workspace AND p.chat=l.id AND p.user_id=$2))
      THEN jsonb_build_object('id',l.id,'name',l.name,'kind',l.kind) END,
    'created_ms',m.created_ms,'changed_ms',m.changed_ms,'used',m.used,'used_ms',m.used_ms,'strength',{STRENGTH},
    'pinned',m.pinned,'essential',m.essential,'until_ms',m.until_ms,'state',m.state,'waiting_reason',m.waiting_reason,
    'replaces',(SELECT jsonb_build_object('id',r.id,'text',r.text,'used',r.used,'created_ms',r.created_ms,'pinned',r.pinned)
      FROM relay_memories r WHERE r.workspace=m.workspace AND r.id=m.replaces))")
}

impl PgStore {
    /// One view of a memory page, with the count of every view.
    /// Views: topics, notes, essentials, fading, archive, waiting, changes, missed.
    pub(crate) async fn memory_page(
        &self,
        user: &User,
        w: &str,
        page: Page<'_>,
        view: &str,
        search: Option<&str>,
        topic: Option<&str>,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        allowed(&tx, user, w, &page).await?;
        if !ready(&tx).await? {
            tx.commit().await?;
            return Ok(json!({"available":false}));
        }
        let base = || -> Vec<sea_orm::Value> {
            vec![w.into(), user.id.clone().into(), page.agent().into()]
        };
        let counts = query(
            &tx,
            &format!("SELECT count(*) FILTER (WHERE {ACTIVE}) AS notes,
                count(DISTINCT m.topic) FILTER (WHERE {ACTIVE} AND m.topic<>'') AS topics,
                count(*) FILTER (WHERE {ACTIVE} AND (m.pinned OR m.essential)) AS essentials,
                count(*) FILTER (WHERE {ACTIVE} AND NOT m.pinned AND {STRENGTH}<{FADING_BELOW}) AS fading,
                count(*) FILTER (WHERE m.state IN ('archived','ended','folded')) AS archive,
                count(*) FILTER (WHERE m.state='waiting') AS waiting,
                COALESCE(sum(m.used) FILTER (WHERE m.used_ms>({NOW_MS}-2592000000)),0)::bigint AS used_month
             {FROM} WHERE m.workspace=$1 AND {ON_PAGE}"),
            base(),
        )
        .await?
        .context("Missing memory counts")?;
        let count = |key: &str| counts.try_get::<i64>("", key).unwrap_or(0);
        let words = search.and_then(|q| any_words(q, 12));
        let mut args = base();
        args.push(words.clone().unwrap_or_default().into());
        args.push(topic.unwrap_or_default().into());
        let matching = "($4='' OR to_tsvector('simple',m.topic||' '||m.text) @@ to_tsquery('simple',$4)) AND ($5='' OR m.topic=$5)";
        let note = note();
        let list = |condition: &str, order: &str, limit: i64| {
            format!(
                "SELECT {note} AS data {FROM} WHERE m.workspace=$1 AND {ON_PAGE} AND {matching} AND {condition} ORDER BY {order} LIMIT {limit}"
            )
        };
        let items = match view {
            "topics" => {
                json_rows(&tx, &format!("WITH n AS (SELECT m.topic,count(*) AS notes,sum(m.used) AS used,max(COALESCE(m.changed_ms,m.created_ms)) AS updated,
                        max({STRENGTH}) AS strength,bool_or(m.pinned) AS pinned,min(m.scope) AS scope,min(c.name) AS room
                      {FROM} WHERE m.workspace=$1 AND {ON_PAGE} AND {ACTIVE} AND m.topic<>'' AND {matching} GROUP BY m.topic)
                    SELECT jsonb_build_object('topic',n.topic,'notes',n.notes,'used',n.used,'updated_ms',n.updated,'strength',n.strength,'pinned',n.pinned,
                      'summary',(SELECT t.summary FROM relay_memory_topics t WHERE t.workspace=$1 AND t.topic=n.topic
                        AND ((t.scope='workspace' AND $3='') OR (t.agent=$3 AND t.scope='job')) LIMIT 1),
                      'lead',(SELECT m.text FROM relay_memories m WHERE m.workspace=$1 AND m.topic=n.topic AND {ACTIVE}
                        AND (($3='' AND m.scope='workspace') OR ($3<>'' AND m.agent=$3)) ORDER BY m.pinned DESC,{STRENGTH} DESC LIMIT 1)) AS data
                    FROM n ORDER BY n.pinned DESC,n.used DESC,n.updated DESC LIMIT 200"), args).await?
            }
            "essentials" => json_rows(&tx, &list(&format!("{ACTIVE} AND {STRENGTH}>={FADING_BELOW}"), &format!("m.pinned DESC,m.essential DESC,{STRENGTH} DESC,m.used DESC"), 30), args).await?,
            "fading" => json_rows(&tx, &list(&format!("{ACTIVE} AND NOT m.pinned AND {STRENGTH}<{FADING_BELOW}"), &format!("{STRENGTH},m.used_ms"), 100), args).await?,
            "archive" => json_rows(&tx, &list("m.state IN ('archived','ended','folded')", "COALESCE(m.changed_ms,m.created_ms) DESC", 100), args).await?,
            "waiting" => json_rows(&tx, &list("m.state='waiting'", "m.created_ms", 100), args).await?,
            "changes" => {
                json_rows(&tx, "SELECT jsonb_build_object('id',x.id,'action',x.action,'summary',x.summary,'at_ms',x.at_ms,'undone_ms',x.undone_ms) AS data
                    FROM relay_memory_changes x WHERE x.workspace=$1 AND (($3='' AND x.scope='workspace') OR ($3<>'' AND x.agent=$3 AND x.scope='job'))
                      AND length($2)>=0 ORDER BY x.at_ms DESC LIMIT 100", base()).await?
            }
            "missed" => {
                json_rows(&tx, "SELECT jsonb_build_object('query',min(u.query),'times',count(*),'agents',jsonb_agg(DISTINCT a.name),'last_ms',max(u.at_ms)) AS data
                    FROM relay_memory_uses u JOIN relay_chats c ON c.workspace=u.workspace AND c.id=u.chat
                    JOIN relay_shared_agents a ON a.workspace=u.workspace AND a.id=u.agent
                    WHERE u.workspace=$1 AND u.action='searched' AND u.found=0 AND ($3='' OR u.agent=$3)
                      AND u.at_ms>(extract(epoch FROM clock_timestamp())*1000)::bigint-2592000000
                      AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2))
                    GROUP BY lower(u.query) ORDER BY count(*) DESC,max(u.at_ms) DESC LIMIT 50", base()).await?
            }
            _ => json_rows(&tx, &list(ACTIVE, "m.pinned DESC,COALESCE(m.changed_ms,m.created_ms) DESC", 200), args).await?,
        };
        tx.commit().await?;
        Ok(json!({"available":true,"view":view,"items":items,"counts":{
            "notes":count("notes"),"topics":count("topics"),"essentials":count("essentials"),"fading":count("fading"),
            "archive":count("archive"),"waiting":count("waiting")},"used_month":count("used_month")}))
    }

    /// What `agent` remembers about the viewer: only ever shown to them.
    pub(crate) async fn memory_about_me(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        if !ready(&tx).await? {
            tx.commit().await?;
            return Ok(json!({"available":false,"notes":[]}));
        }
        let notes = json_rows(
            &tx,
            &format!("SELECT jsonb_build_object('id',m.id,'kind',m.kind,'topic',m.topic,'text',m.text,'created_ms',m.created_ms,'used',m.used) AS data
             FROM relay_memories m WHERE m.workspace=$1 AND m.shelf=$2 AND {ACTIVE} ORDER BY m.created_ms DESC LIMIT 500"),
            vec![w.into(), Shelf::person(agent, &user.id).key().into()],
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"available":true,"notes":notes}))
    }

    /// The viewer removes something an agent remembers about them, for good.
    pub(crate) async fn forget_about_me(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        ensure!(ready(&tx).await?, "Memory is not available yet");
        let removed = execute(
            &tx,
            "DELETE FROM relay_memories WHERE workspace=$1 AND shelf=$2 AND id=$3",
            vec![
                w.into(),
                Shelf::person(agent, &user.id).key().into(),
                id.into(),
            ],
        )
        .await?;
        ensure!(
            removed == 1,
            "That note is not in your memory with this agent"
        );
        tx.commit().await?;
        Ok(())
    }
}
