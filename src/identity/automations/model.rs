use anyhow::{Result, ensure};
/// Ids of saved requests and records are UUIDs.
pub(crate) fn uuid(id: &str) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(id).is_ok(),
        "Invalid routine identity"
    );
    Ok(())
}
