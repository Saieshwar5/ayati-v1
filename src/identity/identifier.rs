//! Stable IDs chosen by people or configuration (servers, event keys, handoffs).
use anyhow::{Result, ensure};

/// Accept short ASCII identifiers that are safe in URLs, logs and SQL text.
pub(crate) fn identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "Invalid service identifier"
    );
    Ok(())
}
