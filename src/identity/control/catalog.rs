//! Private operator-reviewed remote MCP profiles, explicitly bound to one workspace.
use super::model::Operation;
use crate::identity::private_config::read as private_file;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Server {
    pub id: String,
    pub workspace: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub url: String,
    pub bearer_file: Option<String>,
    pub operations: Vec<Operation>,
}
impl Server {
    pub fn fingerprint(&self) -> Result<String> {
        self.fingerprint_for(&self.bearer()?)
    }
    pub fn fingerprint_for(&self, bearer: &Option<String>) -> Result<String> {
        Ok(crate::identity::provider::digest(&format!(
            "{}\n{}",
            serde_json::to_string(self)?,
            bearer.as_deref().unwrap_or_default()
        )))
    }
    pub fn bearer(&self) -> Result<Option<String>> {
        self.bearer_file
            .as_ref()
            .map(|p| {
                let token = private_file(Path::new(p))?;
                let token = token.trim().to_owned();
                ensure!(
                    !token.is_empty()
                        && token.len() <= 4096
                        && !token.chars().any(char::is_control),
                    "Invalid remote credential file"
                );
                Ok(token)
            })
            .transpose()
    }
    pub fn operation(&self, id: &str) -> Result<Operation> {
        self.operations
            .iter()
            .find(|op| op.id == id)
            .cloned()
            .context("Operation is not supported by this connection")
    }
    pub fn public(&self) -> Value {
        json!({"id":self.id,"name":self.name,"account":self.account,"operations":self.operations})
    }
}
pub(crate) fn servers(workspace: &str) -> Result<Vec<Server>> {
    let Some(path) = std::env::var_os("AYATI_MCP_SERVERS_FILE") else {
        return Ok(vec![]);
    };
    let servers: Vec<Server> = serde_json::from_str(&private_file(Path::new(&path))?)
        .map_err(|_| anyhow::anyhow!("Invalid private MCP server configuration"))?;
    ensure!(servers.len() <= 50, "Too many MCP profiles");
    let mut ids = std::collections::HashSet::new();
    for server in &servers {
        ensure!(
            ids.insert((&server.workspace, &server.id)),
            "Duplicate MCP server identity"
        );
        crate::identity::identifier(&server.id)?;
        super::endpoint(&server.url)?;
        ensure!(
            !server.name.trim().is_empty()
                && server.name.len() <= 160
                && server.account.as_ref().is_none_or(|a| !a.trim().is_empty()
                    && a.len() <= 200
                    && !a.chars().any(char::is_control))
                && !server.operations.is_empty()
                && server.operations.len() <= 20,
            "Invalid MCP profile"
        );
        let mut names = std::collections::HashSet::new();
        for op in &server.operations {
            op.validate()?;
            ensure!(names.insert(&op.id), "Duplicate operation identity");
        }
    }
    Ok(servers
        .into_iter()
        .filter(|s| s.workspace == workspace)
        .collect())
}
pub(crate) fn server(workspace: &str, id: &str) -> Result<Server> {
    servers(workspace)?
        .into_iter()
        .find(|s| s.id == id)
        .context("Remote MCP profile is unavailable for this workspace")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operator_files_reject_shared_permissions_and_symlinks() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile");
        std::fs::write(&path, "[]").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(private_file(&path).is_ok());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(private_file(&path).is_err());
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(private_file(&link).is_err());
    }
}
