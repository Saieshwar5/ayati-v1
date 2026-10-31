//! Readable change records for agent settings (D233): "Arjun changed Atlas:
//! instructions; who can talk (Selected people → Everyone)". Activity (M4) shows
//! these sentences; the instructions text itself is never copied into the log.
use super::*;
use crate::identity::agents::config::SaveAgent;

/// What `save_lists` changed besides the agent row.
#[derive(Default)]
pub(super) struct ListChanges {
    pub narrowed: bool,
    pub people: bool,
    pub managers: bool,
}

fn audience(value: &str) -> &'static str {
    match value {
        "everyone" => "Everyone",
        "only_me" => "Only me",
        _ => "Selected people",
    }
}

fn quoted(value: &str) -> String {
    if value.trim().is_empty() {
        "none".into()
    } else {
        format!("“{}”", value.trim())
    }
}

/// One sentence describing a save. `before` is the saved row, or `None` when
/// the agent was just created.
pub(super) fn summary(
    actor: &str,
    before: Option<&Value>,
    after: &SaveAgent,
    lists: &ListChanges,
) -> String {
    let name = after.name.trim();
    let Some(old) = before else {
        return format!("{actor} created {name}");
    };
    let s = &old["settings"];
    let mut parts = Vec::new();
    if old["name"] != name {
        parts.push(format!("name ({} → {})", quoted(old["name"].as_str().unwrap_or_default()), quoted(name)));
    }
    if old["role"] != after.role.trim() {
        parts.push(format!("job title ({} → {})", quoted(old["role"].as_str().unwrap_or_default()), quoted(&after.role)));
    }
    if s["instructions"] != after.settings.instructions.as_str()
        || s["escalation"] != after.settings.escalation.as_str()
    {
        parts.push("instructions".into());
    }
    if old["audience"] != after.audience {
        parts.push(format!("who can talk ({} → {})", audience(old["audience"].as_str().unwrap_or_default()), audience(&after.audience)));
    }
    if lists.people {
        parts.push("selected people".into());
    }
    if lists.managers {
        parts.push("managers".into());
    }
    if old["model"] != after.model {
        parts.push(format!("model ({} → {})", old["model"].as_str().unwrap_or_default(), after.model));
    }
    if s["avatar"] != after.settings.avatar.as_str() {
        parts.push("picture".into());
    }
    if s["memory"] != after.settings.memory {
        parts.push(format!("knowledge {}", if after.settings.memory { "on" } else { "off" }));
    }
    if s["history"] != after.settings.history.as_str() {
        parts.push("channel history".into());
    }
    let paused = s["paused"] == true;
    let mut sentence = match (paused, after.settings.paused) {
        (false, true) => format!("{actor} paused {name}"),
        (true, false) => format!("{actor} resumed {name}"),
        _ if parts.is_empty() => format!("{actor} saved {name} without changes"),
        _ => format!("{actor} changed {name}"),
    };
    if !parts.is_empty() {
        sentence.push_str(&format!("{}{}", if paused == after.settings.paused { ": " } else { " and changed " }, parts.join("; ")));
    }
    sentence.chars().take(1000).collect()
}

/// Writes the audit entry; schemas before 44 (upgrade tests) have no summary.
pub(super) async fn record(
    db: &impl ConnectionTrait,
    actor: &User,
    w: &str,
    agent: &str,
    sentence: Option<String>,
) -> Result<()> {
    match sentence {
        Some(text) if super::agent_audience::exists(db).await? => {
            audit_with_summary(db, &actor.id, Some(w), "agent_profile_saved", agent, &text).await
        }
        _ => audit(db, &actor.id, Some(w), "agent_profile_saved", agent).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::agents::config::Settings;
    use serde_json::json;

    fn saved() -> SaveAgent {
        SaveAgent {
            id: "a".into(),
            name: "Atlas".into(),
            role: "Orders".into(),
            model: "m1".into(),
            audience: "selected".into(),
            people: None,
            managers: None,
            revision: 1,
            settings: Settings::default(),
        }
    }

    #[test]
    fn sentences_name_what_changed_without_copying_instructions() {
        let old = json!({"name":"Atlas","role":"Orders","model":"m1","audience":"selected","settings":serde_json::to_value(Settings::default()).unwrap()});
        let none = ListChanges::default();
        assert_eq!(summary("Arjun", None, &saved(), &none), "Arjun created Atlas");
        assert_eq!(summary("Arjun", Some(&old), &saved(), &none), "Arjun saved Atlas without changes");
        let mut next = saved();
        next.audience = "everyone".into();
        next.settings.instructions = "SECRET PRICE LIST".into();
        next.role = "Order desk".into();
        let lists = ListChanges { managers: true, ..Default::default() };
        let text = summary("Arjun", Some(&old), &next, &lists);
        assert_eq!(text, "Arjun changed Atlas: job title (“Orders” → “Order desk”); instructions; who can talk (Selected people → Everyone); managers");
        assert!(!text.contains("SECRET"));
        let mut paused = saved();
        paused.settings.paused = true;
        assert_eq!(summary("Arjun", Some(&old), &paused, &none), "Arjun paused Atlas");
        paused.model = "m2".into();
        assert_eq!(summary("Arjun", Some(&old), &paused, &none), "Arjun paused Atlas and changed model (m1 → m2)");
    }
}
