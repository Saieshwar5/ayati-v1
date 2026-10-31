//! Settings → Audit log (D231 slice 6): who changed access, connections and
//! approvals, as readable sentences with names, filtered by kind. Admins only.
use super::*;
use serde_json::json;

/// Which audit actions each filter shows. "all" shows every entry.
fn actions(kind: &str) -> Result<Option<&'static [&'static str]>> {
    Ok(match kind {
        "all" => None,
        "people" => Some(&[
            "membership_changed",
            "invitation_created",
            "invitation_accepted",
            "invitation_cancelled",
            "invitation_declined",
            "invitation_link_issued",
            "workspace_code_issued",
            "workspace_code_revoked",
            "workspace_code_used",
            "agent_owner_changed",
        ]),
        "connections" => Some(&[
            "control_connected",
            "control_disconnected",
            "control_connector_rule_saved",
            "control_rule_saved",
            "control_system_approval_saved",
            "control_agent_connection_on",
            "workspace_connection_saved",
        ]),
        "approvals" => Some(&["control_action_decided"]),
        "admin_views" => Some(&["activity_private_viewed"]),
        _ => anyhow::bail!("Invalid audit filter"),
    })
}

/// A sentence for entries saved without one. `{}` is the target's name.
fn phrase(action: &str) -> &'static str {
    match action {
        "membership_changed" => "changed the membership of {}",
        "invitation_created" => "invited {}",
        "invitation_accepted" => "joined the workspace",
        "invitation_cancelled" => "cancelled an invitation",
        "invitation_declined" => "declined an invitation",
        "invitation_link_issued" => "created an invitation link",
        "workspace_code_issued" => "created a workspace code",
        "workspace_code_revoked" => "revoked a workspace code",
        "workspace_code_used" => "joined with a workspace code",
        "workspace_created" => "created the workspace",
        "workspace_renamed" => "renamed the workspace to {}",
        "workspace_closed" => "deleted the workspace",
        "control_connected" => "connected {}",
        "control_disconnected" => "disconnected {}",
        "control_connector_rule_saved" => "changed a safety rule for {}",
        "control_rule_saved" => "changed an agent's rule for a connector",
        "control_agent_connection_on" => "switched a connector on for {}",
        "control_system_approval_saved" => "approved a business system",
        "workspace_connection_saved" => "changed the connection {}",
        "control_action_decided" => "decided an approval request",
        "control_action_requested" => "asked for approval",
        "activity_private_viewed" => {
            "viewed a request from a conversation they are not in (admin view)"
        }
        "shared_agent_created" => "created {}",
        "agent_profile_saved" => "changed {}",
        "channel_members_updated" => "changed the members of {}",
        "channel_agents_updated" => "changed the agents in {}",
        "channel_archived" => "archived {}",
        "channel_settings_updated" => "changed the settings of {}",
        "library_access_changed" => "changed Library access",
        "sign_in" => "signed in",
        "profile_updated" => "updated their profile",
        "profile_photo_updated" => "changed their photo",
        _ => "",
    }
}

/// The sentence after the actor's name: a saved summary without the leading name,
/// or a phrase with the target's name.
fn sentence(actor: &str, action: &str, target: &str, summary: Option<&str>) -> String {
    if let Some(summary) = summary.filter(|s| !s.trim().is_empty()) {
        return summary
            .strip_prefix(actor)
            .map(str::trim_start)
            .unwrap_or(summary)
            .to_string();
    }
    match phrase(action) {
        "" => action.replace('_', " "),
        text => text.replace("{}", target),
    }
}

impl PgStore {
    /// 50 entries, newest first, for one filter; `before` pages back by id.
    pub async fn workspace_activity(
        &self,
        user: &User,
        w: &str,
        kind: &str,
        before: Option<i64>,
    ) -> Result<Value> {
        let only = actions(kind)?;
        ensure!(before.is_none_or(|b| b > 0), "Invalid cursor");
        let tx = self.read_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let list: Option<Vec<String>> = only.map(|a| a.iter().map(|s| s.to_string()).collect());
        let records = rows(&tx, "SELECT a.id,a.action,a.at,to_jsonb(a)->>'summary' AS summary,
              COALESCE(u.display_name,u.email,CASE WHEN a.actor='system' THEN 'Cuckoo' END,'Former member') AS actor,
              COALESCE(tu.display_name,tu.email,ag.name,'#'||ch.name,cn.server,a.target) AS target
            FROM relay_audit a
            LEFT JOIN relay_users u ON u.id=a.actor
            LEFT JOIN relay_users tu ON tu.id=a.target
            LEFT JOIN relay_shared_agents ag ON ag.workspace=a.workspace AND ag.id=a.target
            LEFT JOIN relay_chats ch ON ch.workspace=a.workspace AND ch.id=a.target
            LEFT JOIN relay_control_connections cn ON cn.workspace=a.workspace AND cn.id=a.target
            WHERE a.workspace=$1 AND a.id<$2 AND ($3::text[] IS NULL OR a.action=ANY($3))
            ORDER BY a.id DESC LIMIT 51",
            vec![w.into(), before.unwrap_or(i64::MAX).into(), list.into()]).await?;
        tx.commit().await?;
        let more = records.len() > 50;
        let mut items = vec![];
        for row in records.into_iter().take(50) {
            let (actor, action): (String, String) =
                (row.try_get("", "actor")?, row.try_get("", "action")?);
            let target: String = row.try_get("", "target")?;
            let summary: Option<String> = row.try_get("", "summary")?;
            items.push(json!({
                "id": row.try_get::<i64>("", "id")?, "at": row.try_get::<i64>("", "at")?, "actor": actor,
                "action": action, "kind": kind_of(&action), "sentence": sentence(&actor, &action, &target, summary.as_deref()),
            }));
        }
        let next = if more {
            items.last().map(|i| i["id"].clone())
        } else {
            None
        };
        Ok(json!({"activity": items, "next_before": next}))
    }
}

/// The filter an entry belongs to, so the page can highlight admin views.
fn kind_of(action: &str) -> &'static str {
    for kind in ["people", "connections", "approvals", "admin_views"] {
        if actions(kind)
            .ok()
            .flatten()
            .is_some_and(|list| list.contains(&action))
        {
            return kind;
        }
    }
    "other"
}

#[cfg(test)]
mod tests {
    use super::sentence;

    #[test]
    fn sentences_read_naturally_without_repeating_the_actor() {
        assert_eq!(
            sentence(
                "Arjun Mehta",
                "agent_profile_saved",
                "a1",
                Some("Arjun Mehta paused Atlas")
            ),
            "paused Atlas"
        );
        assert_eq!(
            sentence("Ravi", "control_connected", "Zoho Books", None),
            "connected Zoho Books"
        );
        assert_eq!(
            sentence("Ravi", "something_new", "x", None),
            "something new"
        );
    }
}
