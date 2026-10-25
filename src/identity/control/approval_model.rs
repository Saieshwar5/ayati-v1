use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApproverGrant {
    pub user: String,
    pub connection: String,
    pub operation: String,
    pub enabled: bool,
    pub revision: i64,
    pub connection_revision: i64,
}
