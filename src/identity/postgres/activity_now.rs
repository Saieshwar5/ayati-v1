//! Activity → Now and request details: agent states, work running now, "while
//! you were away", and what one request did step by step (D235).
use super::*;
use serde_json::json;

/// Words in a reply that claim an outside change. Without a completed change in
/// the records, the details panel says the two disagree.
const CLAIMS: [&str; 9] = [
    "sent",
    "emailed",
    "updated",
    "created",
    "deleted",
    "paid",
    "booked",
    "scheduled",
    "cancelled",
];

fn claims_change(reply: &str) -> bool {
    let lower = reply.to_lowercase();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| CLAIMS.contains(&word))
}

impl PgStore {
    /// Records that the person has caught up: "while you were away" then starts
    /// from now. It is set by an explicit action, never by opening the page.
    pub(crate) async fn activity_seen(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        // Millisecond time, as requests record it, so work from this second counts as seen.
        let row = query(&tx, "INSERT INTO relay_activity_visits(workspace,user_id,seen_ms)
            VALUES($1,$2,(extract(epoch FROM clock_timestamp())*1000)::bigint)
            ON CONFLICT(workspace,user_id) DO UPDATE SET seen_ms=GREATEST(relay_activity_visits.seen_ms,EXCLUDED.seen_ms)
            RETURNING seen_ms",
            vec![w.into(), user.id.clone().into()]).await?.context("Visit unavailable")?;
        let since: i64 = row.try_get("", "seen_ms")?;
        tx.commit().await?;
        Ok(json!({"since": since}))
    }

    /// Agent cards, running work and totals since the last visit.
    pub(crate) async fn activity_now(&self, user: &User, w: &str, today_ms: i64) -> Result<Value> {
        let tx = self.activity_tx(w, false).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let none = Filters::default();
        let mut values: Vec<sea_orm::Value> = vec![
            w.into(),
            user.id.clone().into(),
            admin.into(),
            none.agent.clone().into(),
            none.person.clone().into(),
            none.chat.clone().into(),
            none.status.clone().into(),
            0i64.into(),
            i64::MAX.into(),
            Option::<String>::None.into(),
            none.kind.clone().into(),
            i64::MAX.into(),
        ];
        values.push(today_ms.into());
        let agents = json_rows(
            &tx,
            &query_with(include_str!("activity_agents.sql")),
            values,
        )
        .await?;
        let since: Option<i64> = query(
            &tx,
            "SELECT seen_ms FROM relay_activity_visits WHERE workspace=$1 AND user_id=$2",
            vec![w.into(), user.id.clone().into()],
        )
        .await?
        .map(|r| r.try_get("", "seen_ms"))
        .transpose()?
        .flatten();
        tx.commit().await?;
        let since = since.unwrap_or(now() * 1000 - 24 * 3600 * 1000);
        let away = self
            .activity_totals(
                user,
                w,
                &Filters {
                    from: Some(since),
                    ..Filters::default()
                },
            )
            .await?;
        let running = self
            .activity_history(
                user,
                w,
                &Filters {
                    status: Some("active".into()),
                    ..Filters::default()
                },
            )
            .await?;
        Ok(
            json!({"agents": agents, "running": running["items"], "away": {"since": since, "totals": away}}),
        )
    }

    /// One request: the original message, each tool step with how it was allowed,
    /// the reply, a plain fix when it failed, and a warning when the reply claims a
    /// change the records don't show. Viewing a conversation the person isn't in
    /// (as a manager or admin) is recorded in the audit log.
    pub(crate) async fn activity_request(
        &self,
        user: &User,
        w: &str,
        reply: &str,
    ) -> Result<Value> {
        let tx = self.activity_tx(w, true).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let mut values = Filters::default().values_for(w, &user.id, admin);
        values.push(reply.into());
        let row = query(&tx, &query_with(include_str!("activity_feed.sql")), values)
            .await?
            .context("Request unavailable")?;
        let agent: String = row.try_get("", "agent")?;
        let agent_name: String = row.try_get("", "agent_name")?;
        let in_conversation: bool = row.try_get("", "in_conversation")?;
        let conversation: String = row.try_get("", "conversation_name")?;
        let reply_text: String = row.try_get("", "content")?;
        let source: Option<String> = row.try_get("", "source_message")?;
        let mut item = super::super::agent_requests::project(&tx, user, w, &agent, row)
            .await?
            .context("Request unavailable")?;
        let message = match source {
            Some(id) => query(&tx,"SELECT left(content,4000) AS content FROM relay_chat_messages WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?.map(|r| r.try_get::<String>("","content")).transpose()?,
            None => None,
        };
        let steps = json_rows(&tx,"SELECT jsonb_build_object('id',a.id,'title',a.title,'effect',a.effect,'state',CASE WHEN a.state='executing' AND a.updated<floor(extract(epoch FROM clock_timestamp()))::bigint-60 THEN 'uncertain' ELSE a.state END,
            'automatic',NOT a.requires_approval,'decision',a.decision,'decided_by',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=a.decided_by),
            'account',(SELECT c.definition->>'name' FROM relay_control_connections c WHERE c.workspace=a.workspace AND c.id=a.connection),
            'arguments',a.arguments,'at',a.created) AS data FROM relay_control_actions a WHERE a.workspace=$1 AND a.job=$2 ORDER BY a.created,a.id",vec![w.into(),reply.into()]).await?;
        let changed = steps
            .iter()
            .any(|s| s["effect"] == "write" && s["state"] == "completed");
        let state = item["state"].as_str().unwrap_or_default().to_string();
        let fix = match state.as_str() {
            "failed" | "interrupted" => Some(format!(
                "{agent_name} could not finish this request. Read its reply above, fix what it needs (for example access to a Library item or a tool on its Tools tab), then ask again in the conversation."
            )),
            "access_changed" => Some(format!(
                "Access changed while {agent_name} was working. Check who can talk to {agent_name} and its tools, then ask again."
            )),
            "cancelled" => Some(
                "This request was stopped. Ask again in the conversation if it is still needed."
                    .to_string(),
            ),
            _ => None,
        };
        let disagreement = (state == "complete" && claims_change(&reply_text) && !changed).then(|| {
            format!("{agent_name}'s reply says something was done, but Cuckoo has no record of a change through its tools. Check before relying on it.")
        });
        if !in_conversation && item["requester"] != user.id {
            let actor = super::super::member_handover::name(&tx, &user.id).await?;
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "activity_private_viewed",
                reply,
                &format!("{actor} viewed a request in {conversation} from Activity"),
            )
            .await?;
        }
        if let Some(object) = item.as_object_mut() {
            object.insert("message".into(), json!(message));
            object.insert(
                "reply".into(),
                json!(reply_text.chars().take(8000).collect::<String>()),
            );
            object.insert("steps".into(), json!(steps));
            object.insert("fix".into(), json!(fix));
            object.insert("disagreement".into(), json!(disagreement));
            object.insert("agent_name".into(), json!(agent_name));
            object.insert("in_conversation".into(), json!(in_conversation));
        }
        tx.commit().await?;
        Ok(item)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn replies_that_claim_a_change_are_found_by_whole_words() {
        assert!(super::claims_change("I sent the invoice to Mrs Kapoor."));
        assert!(super::claims_change("Order UPDATED."));
        assert!(!super::claims_change(
            "I could not send it; the presentation is ready."
        ));
        assert!(!super::claims_change("Here is the draft."));
    }
}
