//! Explicit dispatch selection; assignment policy is fixed when work is accepted.
use anyhow::{Context, Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mode {
    Embedded,
    External,
    Assigned,
}
impl Mode {
    pub fn parse(value: Option<&str>, postgres: bool) -> Result<Self> {
        let mode = match value.unwrap_or("embedded") {
            "embedded" => Self::Embedded,
            "external" => Self::External,
            "assigned" => Self::Assigned,
            _ => anyhow::bail!("AYATI_CHAT_EXECUTION must be embedded, external or assigned"),
        };
        ensure!(
            postgres || mode == Self::Embedded,
            "External/assigned chat execution requires RELAY_DATABASE_URL"
        );
        Ok(mode)
    }
    pub fn from_env(postgres: bool) -> Result<Self> {
        Self::parse(
            std::env::var("AYATI_CHAT_EXECUTION").ok().as_deref(),
            postgres,
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
        assert!(Mode::parse(Some("assigned"), false).is_err());
        assert_eq!(Mode::parse(Some("assigned"), true).unwrap(), Mode::Assigned);
    }
}
