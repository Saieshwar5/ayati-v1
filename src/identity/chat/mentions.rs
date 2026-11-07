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

/// A person's message asks the agent it mentions, so a mentioned agent must be
/// the message's target.
pub(crate) fn ensure_target(mentions: &[(String, String)], target: Option<&str>) -> Result<()> {
    for (kind, id) in mentions {
        if kind != "agent" {
            continue;
        }
        match target {
            None => bail!(
                "This message mentions an agent without asking it. Ask that agent, or remove the mention."
            ),
            Some(target) if target != id => bail!("Ask one mentioned agent per message"),
            Some(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sent(content: &str, target: Option<&str>) -> Result<Vec<(String, String)>> {
        let mentions = parse(content)?;
        ensure_target(&mentions, target)?;
        Ok(mentions)
    }
    #[test]
    fn mentions_keep_identity_separate_from_names_and_authority() {
        assert_eq!(
            sent(
                "[@Same name](mention:user:u1) [@Same name](mention:user:u2)",
                None
            )
            .unwrap()
            .len(),
            2
        );
        let untargeted = sent("[@Agent](mention:agent:a1)", None).unwrap_err();
        assert!(untargeted.to_string().contains("without asking it"));
        let other = sent("[@Agent](mention:agent:a1)", Some("a2")).unwrap_err();
        assert!(other.to_string().contains("one mentioned agent"));
        assert!(sent("[@Agent](mention:agent:a1)", Some("a1")).is_ok());
        assert!(sent("[@Nobody](mention:user:../../secret)", None).is_err());
        assert!(sent("ordinary @text", None).unwrap().is_empty());
        // Form alone: an agent mention parses without a target.
        assert_eq!(
            parse("Ask [@Atlas](mention:agent:a1)").unwrap(),
            vec![("agent".to_string(), "a1".to_string())]
        );
    }
}
