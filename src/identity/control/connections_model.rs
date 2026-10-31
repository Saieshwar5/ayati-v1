use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SystemApproval {
    pub server: String,
    pub approved: bool,
    pub revision: i64,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceConnection {
    pub id: String,
    pub server: String,
    pub enabled: bool,
    pub revision: i64,
}

/// The admin's rule for one connector action (Settings → Connections).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConnectorRule {
    pub connection: String,
    pub operation: String,
    pub mode: String,
    #[serde(default)]
    pub needs_other: bool,
    pub revision: i64,
}
