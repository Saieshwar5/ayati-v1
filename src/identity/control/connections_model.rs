use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SystemApproval {
    pub server: String,
    pub approved: bool,
    pub revision: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceConnection {
    pub id: String,
    pub server: String,
    pub enabled: bool,
    pub revision: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Delegation {
    pub user: String,
    pub connection: String,
    pub operation: String,
    pub ceiling: String,
    pub revision: i64,
    pub connection_revision: i64,
}
