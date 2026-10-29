//! Collects every configuration problem before failing, so one restart fixes them all.
use anyhow::Result;
use std::{collections::HashMap, fmt::Display, ops::RangeInclusive, str::FromStr};

/// Every setting a Cuckoo process reads. Others with these prefixes are typos.
const KNOWN: &[&str] = &[
    "AYATI_ARCHIVE_LOCAL_ROOT",
    "AYATI_ARCHIVE_S3_BUCKET",
    "AYATI_ARCHIVE_S3_ENDPOINT",
    "AYATI_ASSIGNMENT_LEASE_SECONDS",
    "AYATI_ASSIGNMENT_RUN_SECONDS",
    "AYATI_AWS_EXECUTOR_CLUSTER_ARN",
    "AYATI_AWS_EXECUTOR_RELEASE_ARN",
    "AYATI_AWS_EXECUTOR_ROLE_ARN",
    "AYATI_AWS_TEST_ENDPOINT",
    "AYATI_BIND_ADDRESS",
    "AYATI_CHAT_EXECUTION",
    "AYATI_CLOUD_WORKERS",
    "AYATI_CONTROL_ADDRESS",
    "AYATI_CONTROL_TOKEN",
    "AYATI_DATA_DIR",
    "AYATI_DOTENV",
    "AYATI_DRAIN_SECONDS",
    "AYATI_EVENT_RECEIPT_LIMIT",
    "AYATI_EVENT_SOURCES_FILE",
    "AYATI_EXECUTOR_CAPACITY",
    "AYATI_EXECUTOR_CHAT_RESERVE",
    "AYATI_EXECUTOR_EXPIRY_SECONDS",
    "AYATI_EXECUTOR_IDENTITY",
    "AYATI_EXECUTOR_MODEL_LIMIT",
    "AYATI_EXECUTOR_SLOTS",
    "AYATI_EXECUTOR_WORKSPACE_LIMIT",
    "AYATI_GOOSE_BIN",
    "AYATI_GOOSE_PROVIDER",
    "AYATI_HANDOFF_WAIT_SECONDS",
    "AYATI_IMAGE",
    "AYATI_LOCAL_PORT",
    "AYATI_LOG_FORMAT",
    "AYATI_MCP_SERVERS_FILE",
    "AYATI_METRICS_SECONDS",
    "AYATI_MODEL",
    "AYATI_PORT",
    "AYATI_PREPARED_GOOSE_BIN",
    "AYATI_PUBLIC_ORIGIN",
    "AYATI_RELEASE",
    "AYATI_RETAIN_ATTEMPT_DAYS",
    "AYATI_RETAIN_SYNC_DAYS",
    "AYATI_SHUTDOWN_SECONDS",
    "AYATI_TRACE_START",
    "AYATI_WORKER_CONTROL_CA",
    "AYATI_WORKER_CONTROL_KEY_FILE",
    "AYATI_WORKER_CONTROL_LISTEN",
    "AYATI_WORKER_CONTROL_TLS_CERT",
    "AYATI_WORKER_CONTROL_TLS_KEY",
    "AYATI_WORKER_CONTROL_URL",
    "AYATI_WORKER_PROVIDERS",
    "CUCKOO_LIBRARY_LOCAL_ROOT",
    "CUCKOO_LIBRARY_S3_BUCKET",
    "CUCKOO_LIBRARY_S3_ENDPOINT",
    "RELAY_ADMIN_DATABASE_URL",
    "RELAY_DATABASE_URL",
    "RELAY_SIMULATION_DATABASE_URL",
    "RELAY_TEST_DATABASE_URL",
];
const PREFIXES: &[&str] = &["AYATI_", "RELAY_", "CUCKOO_"];

/// A snapshot of the environment taken once at startup.
pub(crate) struct Reader {
    values: HashMap<String, String>,
    problems: Vec<String>,
}

impl Reader {
    pub(crate) fn process() -> Self {
        Self {
            values: std::env::vars_os()
                .filter_map(|(name, value)| {
                    Some((name.into_string().ok()?, value.into_string().ok()?))
                })
                .collect(),
            problems: vec![],
        }
    }
    #[cfg(test)]
    pub(crate) fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            values: pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            problems: vec![],
        }
    }
    pub(crate) fn problem(&mut self, message: impl Display) {
        self.problems.push(message.to_string());
    }
    /// A non-empty value, trimmed of surrounding whitespace.
    pub(crate) fn text(&self, name: &str) -> Option<String> {
        self.values
            .get(name)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    }
    pub(crate) fn present(&self, name: &str) -> bool {
        self.text(name).is_some()
    }
    pub(crate) fn required(&mut self, name: &str) -> Option<String> {
        let value = self.text(name);
        if value.is_none() {
            self.problem(format!("Set {name}"));
        }
        value
    }
    pub(crate) fn number<T>(&mut self, name: &str, default: T, range: RangeInclusive<T>) -> T
    where
        T: FromStr + PartialOrd + Display + Copy,
    {
        let Some(raw) = self.text(name) else {
            return default;
        };
        match raw.parse::<T>() {
            Ok(value) if range.contains(&value) => value,
            _ => {
                self.problem(format!(
                    "{name} must be a number from {} to {}",
                    range.start(),
                    range.end()
                ));
                default
            }
        }
    }
    /// `1` enables, `0` or unset disables; anything else is a problem.
    pub(crate) fn flag(&mut self, name: &str) -> bool {
        match self.text(name).as_deref() {
            None | Some("0") => false,
            Some("1") => true,
            Some(_) => {
                self.problem(format!("{name} must be 0 or 1"));
                false
            }
        }
    }
    /// Secrets and settings that belong to another process must not be set here.
    pub(crate) fn refuse(&mut self, names: &[&str], role: &str) {
        for name in names {
            if self.present(name) {
                self.problem(format!("{role} must not receive {name}"));
            }
        }
    }
    /// Fail with every problem at once; warn about unknown names (never values).
    pub(crate) fn finish(&self, role: &str) -> Result<()> {
        for name in self.values.keys() {
            if PREFIXES.iter().any(|p| name.starts_with(p)) && !KNOWN.contains(&name.as_str()) {
                tracing::warn!(setting = %name, "Unknown setting ignored; check for a typo");
            }
        }
        anyhow::ensure!(
            self.problems.is_empty(),
            "Invalid {role} configuration:\n- {}",
            self.problems.join("\n- ")
        );
        Ok(())
    }
}
