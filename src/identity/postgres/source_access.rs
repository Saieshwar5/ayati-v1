//! Existing Library and connected-event data retain their independent provenance.
use super::*;
use crate::identity::chat::store::Job;
pub(super) async fn allowed(db: &impl ConnectionTrait, job: &Job) -> Result<bool> {
    Ok(super::library::source_allowed(db, job).await?
        && super::event_access::allowed(db, job).await?
        && super::handoff_access::source_allowed(db, job).await?)
}
