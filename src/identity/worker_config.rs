//! Explicit dispatch selection; assignment policy is fixed when work is accepted.
use anyhow::{Context, Result, ensure};

/// How agent replies run. Controllers always dispatch to pooled executors; only
/// the company simulation may answer with scripted replies inside the controller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mode {
    Pooled,
    Scripted,
}
impl Mode {
    pub fn parse(value: Option<&str>, simulation: bool) -> Result<Self> {
        Ok(match (value, simulation) {
            (None, false) | (Some("pooled"), _) => Self::Pooled,
            (None, true) | (Some("scripted"), true) => Self::Scripted,
            (Some("scripted"), false) => {
                anyhow::bail!("Scripted replies are only available in ayati simulate")
            }
            _ => anyhow::bail!("AYATI_CHAT_EXECUTION must be pooled (or scripted in a simulation)"),
        })
    }
    pub fn from_env(simulation: bool) -> Result<Self> {
        Self::parse(
            std::env::var("AYATI_CHAT_EXECUTION").ok().as_deref(),
            simulation,
        )
    }
}
pub(crate) fn seconds(name: &str, default: u64, min: u64, max: u64) -> Result<u64> {
    parse_seconds(name, std::env::var(name).ok().as_deref(), default, min, max)
}
fn parse_seconds(name: &str, value: Option<&str>, default: u64, min: u64, max: u64) -> Result<u64> {
    let value = value
        .map(str::parse::<u64>)
        .transpose()
        .with_context(|| format!("{name} must be an integer from {min} to {max}"))?
        .unwrap_or(default);
    ensure!(
        (min..=max).contains(&value),
        "{name} must be from {min} to {max}"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assignment_policy_is_bounded() {
        assert_eq!(parse_seconds("duration", None, 180, 1, 86400).unwrap(), 180);
        assert_eq!(
            parse_seconds("duration", Some("21600"), 180, 1, 86400).unwrap(),
            21600
        );
        for value in ["0", "86401", "-1", "forever"] {
            assert!(parse_seconds("duration", Some(value), 180, 1, 86400).is_err());
        }
    }
    #[test]
    fn controllers_only_run_pooled_and_simulations_default_to_scripted() {
        assert_eq!(Mode::parse(None, false).unwrap(), Mode::Pooled);
        assert_eq!(Mode::parse(Some("pooled"), true).unwrap(), Mode::Pooled);
        assert_eq!(Mode::parse(None, true).unwrap(), Mode::Scripted);
        assert!(Mode::parse(Some("scripted"), false).is_err());
        for retired in ["embedded", "external", "assigned"] {
            assert!(Mode::parse(Some(retired), false).is_err());
        }
    }
}
