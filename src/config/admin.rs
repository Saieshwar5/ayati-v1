//! `ayati admin`: Cuckoo Control from a terminal. It uses the runtime database
//! role like the controller, and acts as one platform administrator named by
//! `AYATI_OPERATOR` (or `--as`), who must be in `AYATI_PLATFORM_ADMINS` and
//! have signed in to Cuckoo at least once.
use super::{absolute, reader::Reader};
use anyhow::Result;
use std::path::PathBuf;

pub struct Admin {
    pub database_url: String,
    pub operator: Option<String>,
    pub platform_admins: Vec<String>,
    pub model_providers_file: Option<PathBuf>,
    pub model_catalog_file: Option<PathBuf>,
    pub secrets_key_file: Option<PathBuf>,
    /// Connector providers, as for the controller: `ayati admin connectors verify`.
    pub mcp_servers_file: Option<PathBuf>,
    pub connectors: super::connectors::Connectors,
}

impl Admin {
    pub fn from_env() -> Result<Self> {
        let mut env = Reader::process();
        let config = Self {
            database_url: env.required("RELAY_DATABASE_URL").unwrap_or_default(),
            operator: env.text("AYATI_OPERATOR").map(|e| e.to_lowercase()),
            platform_admins: super::operators::platform_admins(&mut env),
            model_providers_file: absolute(&mut env, "AYATI_MODEL_PROVIDERS_FILE"),
            model_catalog_file: absolute(&mut env, "AYATI_MODEL_CATALOG_FILE"),
            secrets_key_file: absolute(&mut env, "AYATI_SECRETS_KEY_FILE"),
            mcp_servers_file: absolute(&mut env, "AYATI_MCP_SERVERS_FILE"),
            connectors: super::connectors::Connectors::read(&mut env),
        };
        env.finish("admin")?;
        Ok(config)
    }
}
