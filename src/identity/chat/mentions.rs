//! Stable mention identities travel in Markdown links; names are presentation only.
use anyhow::{Result, ensure};

pub(crate) fn parse(content: &str, target: Option<&str>) -> Result<Vec<(String, String)>> {
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
        if kind == "agent" {
            ensure!(target == Some(id), "Ask one mentioned agent per message");
        }
        let mention = (kind.to_string(), id.to_string());
        if !mentions.contains(&mention) {
            mentions.push(mention);
        }
        ensure!(mentions.len() <= 30, "Mention at most 30 participants");
    }
    Ok(mentions)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mentions_keep_identity_separate_from_names_and_authority() {
        assert_eq!(
            parse(
                "[@Same name](mention:user:u1) [@Same name](mention:user:u2)",
                None
            )
            .unwrap()
            .len(),
            2
        );
        assert!(parse("[@Agent](mention:agent:a1)", None).is_err());
        assert!(parse("[@Agent](mention:agent:a1)", Some("a2")).is_err());
        assert!(parse("[@Agent](mention:agent:a1)", Some("a1")).is_ok());
        assert!(parse("[@Nobody](mention:user:../../secret)", None).is_err());
        assert!(parse("ordinary @text", None).unwrap().is_empty());
    }
}
