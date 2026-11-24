//! How each answer landed. Plain signals decide first: a refused approval is a
//! correction, a person's reaction with no reply is an acceptance, and silence
//! for six hours is "not known". When the asker wrote back in the same thread,
//! the classifier reads the answer and their message. Saving an outcome changes
//! the notes the request used.
use super::*;

/// Reactions that mean the answer helped, as agent learning reads them.
const THANKS: [&str; 8] = ["👍", "✅", "🙏", "👏", "🎉", "💯", "❤️", "🔥"];
/// How long the asker has to answer before the outcome is "not known".
const WAIT_S: i64 = 6 * 3600;

/// A finished answer without an outcome.
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub workspace: String,
    pub job: String,
    pub agent: String,
    pub chat: String,
    pub answer: String,
    pub at_s: i64,
    /// The asker's next message in the same thread: its id and text.
    pub next: Option<(String, String)>,
    pub reaction: Option<String>,
    pub refused: bool,
}

/// What plain signals decide, or `None` when the classifier must read the
/// reply (or it is too early to tell).
pub(crate) fn by_rules(
    c: &Candidate,
    now_s: i64,
    classifier: bool,
) -> Option<(&'static str, f64, &'static str)> {
    if c.refused {
        return Some(("corrected", 1.0, "approval"));
    }
    if c.next.is_some() {
        return (!classifier && c.at_s + WAIT_S < now_s).then_some(("unknown", 1.0, "time"));
    }
    if let Some(emoji) = &c.reaction {
        if THANKS.contains(&emoji.as_str()) {
            return Some(("accepted", 0.9, "reaction"));
        }
        return Some(("followed_up", 0.6, "reaction"));
    }
    (c.at_s + WAIT_S < now_s).then_some(("unknown", 1.0, "time"))
}

impl PgStore {
    /// Finished answers of the last two weeks with no outcome yet, oldest first.
    pub(crate) async fn outcome_candidates(&self, limit: i64) -> Result<Vec<Candidate>> {
        if !ready(&self.db).await? {
            return Ok(vec![]);
        }
        let found = rows(
            &self.db,
            "SELECT j.workspace,j.reply,m.author AS agent,m.chat,left(m.content,1500) AS answer,m.at,
               (SELECT jsonb_build_array(f.id,left(f.content,800)) FROM relay_chat_messages f WHERE f.workspace=m.workspace AND f.chat=m.chat
                 AND f.kind='user' AND f.author=j.payload->>'requester' AND f.sequence>m.sequence AND f.thread IS NOT DISTINCT FROM m.thread
                 AND f.state IN ('sent','complete') AND f.at<=m.at+21600 ORDER BY f.sequence LIMIT 1) AS next,
               (SELECT r.emoji FROM relay_message_reactions r WHERE r.workspace=m.workspace AND r.message=m.id ORDER BY r.at LIMIT 1) AS reaction,
               EXISTS(SELECT 1 FROM relay_control_actions x WHERE x.workspace=j.workspace AND x.job=j.reply AND x.state='rejected') AS refused
             FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
             JOIN relay_shared_agents a ON a.workspace=m.workspace AND a.id=m.author AND a.revision>0
             LEFT JOIN relay_request_outcomes o ON o.workspace=j.workspace AND o.job=j.reply
             WHERE o.job IS NULL AND j.state='finished' AND m.kind='agent' AND m.state='complete'
               AND m.at>floor(extract(epoch FROM clock_timestamp()))::bigint-14*86400
             ORDER BY m.at LIMIT $1",
            vec![limit.into()],
        )
        .await?;
        found
            .into_iter()
            .map(|r| {
                let next: Option<Value> = r.try_get("", "next")?;
                Ok(Candidate {
                    workspace: r.try_get("", "workspace")?,
                    job: r.try_get("", "reply")?,
                    agent: r.try_get("", "agent")?,
                    chat: r.try_get("", "chat")?,
                    answer: r.try_get("", "answer")?,
                    at_s: r.try_get("", "at")?,
                    next: next.map(|n| {
                        (
                            n[0].as_str().unwrap_or_default().to_owned(),
                            n[1].as_str().unwrap_or_default().to_owned(),
                        )
                    }),
                    reaction: r.try_get("", "reaction")?,
                    refused: r.try_get("", "refused")?,
                })
            })
            .collect()
    }

    /// Saves one outcome once and applies it to the notes the request used: an
    /// accepted answer makes them stronger and restarts their clock; a corrected
    /// one makes them weaker. Returns the notes a correction may contradict.
    pub(crate) async fn save_outcome(
        &self,
        c: &Candidate,
        outcome: &str,
        confidence: f64,
        source: &str,
        model: Option<&str>,
    ) -> Result<Vec<(String, String)>> {
        let w = &c.workspace;
        let tx = self.workspace_tx(w).await?;
        let saved = execute(
            &tx,
            "INSERT INTO relay_request_outcomes(workspace,job,agent,chat,outcome,confidence,source,message,model) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
            vec![
                w.clone().into(),
                c.job.clone().into(),
                c.agent.clone().into(),
                c.chat.clone().into(),
                outcome.into(),
                (confidence as f32).into(),
                source.into(),
                c.next.as_ref().map(|n| n.0.clone()).into(),
                model.map(str::to_owned).into(),
            ],
        )
        .await?;
        let mut used = vec![];
        if saved == 1 && confidence >= ACT && super::super::memory::ready(&tx).await? {
            let change = match outcome {
                "accepted" => Some(format!(
                    "strength=least(100,strength+5),used_ms={}",
                    super::super::memory::NOW_MS
                )),
                "corrected" => Some("strength=greatest(0,strength-10)".to_owned()),
                _ => None,
            };
            if let Some(change) = change {
                execute(
                    &tx,
                    &format!("UPDATE relay_memories m SET {change},changed_ms={} FROM relay_memory_used u
                      WHERE u.workspace=$1 AND u.job=$2 AND NOT u.settled AND m.workspace=u.workspace AND m.id=u.memory AND m.state='active'",
                        super::super::memory::NOW_MS),
                    vec![w.clone().into(), c.job.clone().into()],
                )
                .await?;
            }
            if outcome == "corrected" {
                used = rows(
                    &tx,
                    "SELECT m.id,m.text FROM relay_memory_used u JOIN relay_memories m ON m.workspace=u.workspace AND m.id=u.memory
                     WHERE u.workspace=$1 AND u.job=$2 AND m.state='active' AND NOT m.pinned ORDER BY m.id LIMIT 12",
                    vec![w.clone().into(), c.job.clone().into()],
                )
                .await?
                .into_iter()
                .map(|r| Ok((r.try_get("", "id")?, r.try_get("", "text")?)))
                .collect::<Result<_>>()?;
            }
            execute(
                &tx,
                "UPDATE relay_memory_used SET settled=true WHERE workspace=$1 AND job=$2",
                vec![w.clone().into(), c.job.clone().into()],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(used)
    }

    /// Puts the notes a correction contradicted on hold: not given or found
    /// until an admin or owner keeps or ends them.
    pub(crate) async fn hold_notes(&self, w: &str, job: &str, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let tx = self.workspace_tx(w).await?;
        let held = execute(
            &tx,
            &format!("UPDATE relay_memories m SET state='waiting',waiting_reason='corrected',changed_ms={},
               check_result=jsonb_build_object('held_by',$2::text)
             FROM relay_memory_used u WHERE u.workspace=$1 AND u.job=$2 AND m.workspace=u.workspace AND m.id=u.memory
               AND m.state='active' AND NOT m.pinned AND m.id=ANY(string_to_array($3,','))", super::super::memory::NOW_MS),
            vec![w.into(), job.into(), ids.join(",").into()],
        )
        .await?;
        tx.commit().await?;
        Ok(held)
    }
}

/// The outcome line of one request in Activity, by the person who showed it.
pub(crate) async fn steps(db: &impl ConnectionTrait, w: &str, reply: &str) -> Result<Vec<Value>> {
    if !ready(db).await? {
        return Ok(vec![]);
    }
    json_rows(db, "SELECT jsonb_build_object('id','outcome-'||o.job,'title',
        CASE o.outcome WHEN 'accepted' THEN 'Outcome: accepted' WHEN 'corrected' THEN 'Outcome: corrected'
          WHEN 'followed_up' THEN 'Outcome: followed up' ELSE 'Outcome: not known' END
        ||CASE o.source WHEN 'reaction' THEN ' (a reaction)' WHEN 'approval' THEN ' (an approval was refused)'
          WHEN 'classifier' THEN COALESCE(' by '||COALESCE(u.display_name,u.email),'') ELSE '' END,
        'effect','outcome','state','completed','automatic',true,'decision',NULL,'decided_by',NULL,'account',NULL,
        'arguments','{}'::jsonb,'at',o.decided_ms/1000) AS data
      FROM relay_request_outcomes o LEFT JOIN relay_chat_messages f ON f.workspace=o.workspace AND f.id=o.message
      LEFT JOIN relay_users u ON u.id=f.author WHERE o.workspace=$1 AND o.job=$2", vec![w.into(), reply.into()]).await
}
