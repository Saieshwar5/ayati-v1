//! Stable mention identities travel in Markdown links; names are presentation only.
use anyhow::{Result, bail, ensure};

/// Every mention in `content`, without repeats. Checks only their form; callers
/// check who may be mentioned.
pub(crate) fn parse(content: &str) -> Result<Vec<(String, String)>> {
    let mut mentions = Vec::new();
    for part in content.split("](mention:").skip(1) {
        let Some((value, _)) = part.split_once(')') else {
            continue;
        };
        let Some((kind, id)) = value.split_once(':') else {
            continue;
        };
        ensure!(matches!(kind, "user" | "agent"), "Unknown mention type");
        ensure!(
            !id.is_empty()
                && id.len() <= 100
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "Invalid mention identity"
        );
        let mention = (kind.to_string(), id.to_string());
        if !mentions.contains(&mention) {
            mentions.push(mention);
        }
        ensure!(mentions.len() <= 30, "Mention at most 30 participants");
    }
    Ok(mentions)
}

/// A person's message asks the agents it mentions, so every mentioned agent
/// must be one of the message's targets. A target need not be mentioned: a
/// direct message, a quoted agent message and an agent's thread ask implicitly.
pub(crate) fn ensure_target(mentions: &[(String, String)], targets: &[String]) -> Result<()> {
    for (kind, id) in mentions {
        if kind != "agent" {
            continue;
        }
        if targets.is_empty() {
            bail!(
                "This message mentions an agent without asking it. Ask that agent, or remove the mention."
            );
        }
        ensure!(
            targets.contains(id),
            "This message mentions an agent it doesn't ask. Ask every agent you mention, or remove the mention."
        );
    }
    Ok(())
}

/// `text` with its mentions turned into plain names, except those `keep`
/// accepts by kind and identity. A plain name asks and notifies nobody.
pub(crate) fn plain(text: &str, keep: impl Fn(&str, &str) -> bool) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("](mention:") {
        let Some(open) = rest[..start].rfind('[') else {
            break;
        };
        let Some(close) = rest[start..].find(')') else {
            break;
        };
        let identity = &rest[start + "](mention:".len()..start + close];
        let kept = identity
            .split_once(':')
            .is_some_and(|(kind, id)| keep(kind, id));
        out.push_str(&rest[..open]);
        out.push_str(if kept {
            &rest[open..start + close + 1]
        } else {
            &rest[open + 1..start]
        });
        rest = &rest[start + close + 1..];
    }
    out.push_str(rest);
    out
}

/// The name `text` shows for an agent it mentions; the person chose it, so it
/// can name that agent back to them without revealing anything new.
pub(crate) fn agent_name(text: &str, agent: &str) -> Option<String> {
    let link = format!("](mention:agent:{agent})");
    let end = text.find(&link)?;
    let open = text[..end].rfind("[@")?;
    Some(text[open + 2..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sent(content: &str, targets: &[&str]) -> Result<Vec<(String, String)>> {
        let mentions = parse(content)?;
        let targets: Vec<String> = targets.iter().map(|t| t.to_string()).collect();
        ensure_target(&mentions, &targets)?;
        Ok(mentions)
    }
    #[test]
    fn mentions_keep_identity_separate_from_names_and_authority() {
        assert_eq!(
            sent(
                "[@Same name](mention:user:u1) [@Same name](mention:user:u2)",
                &[]
            )
            .unwrap()
            .len(),
            2
        );
        let untargeted = sent("[@Agent](mention:agent:a1)", &[]).unwrap_err();
        assert!(untargeted.to_string().contains("without asking it"));
        let other = sent("[@Agent](mention:agent:a1)", &["a2"]).unwrap_err();
        assert!(other.to_string().contains("doesn't ask"));
        assert!(sent("[@Agent](mention:agent:a1)", &["a1"]).is_ok());
        assert!(sent("[@Nobody](mention:user:../../secret)", &[]).is_err());
        assert!(sent("ordinary @text", &[]).unwrap().is_empty());
        // Form alone: an agent mention parses without a target.
        assert_eq!(
            parse("Ask [@Atlas](mention:agent:a1)").unwrap(),
            vec![("agent".to_string(), "a1".to_string())]
        );
    }
    #[test]
    fn every_mentioned_agent_is_asked_and_targets_may_be_implicit() {
        let both = "[@Atlas](mention:agent:a1) and [@Nova](mention:agent:n1), stock?";
        assert!(sent(both, &["a1", "n1"]).is_ok());
        assert!(sent(both, &["n1", "a1"]).is_ok(), "order doesn't matter");
        let missing = sent(both, &["a1"]).unwrap_err();
        assert!(missing.to_string().contains("Ask every agent you mention"));
        // A direct message's agent or a quoted agent is asked without a mention.
        assert!(sent("[@Atlas](mention:agent:a1) too", &["q1", "a1"]).is_ok());
        assert!(sent("No mention", &["q1"]).is_ok());
    }
    #[test]
    fn mentions_become_plain_names_except_those_kept() {
        let text =
            "Hi [@Sam](mention:user:u1), [@Atlas](mention:agent:a1) and [@Nova](mention:agent:n1)?";
        assert_eq!(plain(text, |_, _| false), "Hi @Sam, @Atlas and @Nova?");
        assert_eq!(
            plain(text, |kind, id| kind != "agent" || id == "a1"),
            "Hi [@Sam](mention:user:u1), [@Atlas](mention:agent:a1) and @Nova?"
        );
        assert_eq!(plain("no mentions", |_, _| false), "no mentions");
        assert_eq!(agent_name(text, "n1").as_deref(), Some("Nova"));
        assert_eq!(agent_name(text, "u1"), None, "people are not agents");
        assert_eq!(agent_name(text, "x9"), None);
    }
}
