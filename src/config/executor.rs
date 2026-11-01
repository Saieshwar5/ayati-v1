//! `ayati executor`: prepared Goose slots that reach the controller only through
//! worker control and never hold database, identity or connector credentials.
use super::{Common, GooseSettings, Reader, WorkerControl};
use anyhow::Result;
use std::path::PathBuf;

/// Controller-only secrets and settings an executor must never be given.
const CONTROLLER_ONLY: &[&str] = &[
    "RELAY_DATABASE_URL",
    "RELAY_ADMIN_DATABASE_URL",
    "RELAY_SIMULATION_DATABASE_URL",
    "WORKOS_API_KEY",
    "WORK_OS_API_KEY",
    "AYATI_MCP_SERVERS_FILE",
    "AYATI_COMPOSIO_KEY_FILE",
    "COMPOSIO_API_KEY",
    "AYATI_EVENT_SOURCES_FILE",
    "CUCKOO_LIBRARY_S3_BUCKET",
    "CUCKOO_LIBRARY_LOCAL_ROOT",
    "AYATI_ARCHIVE_S3_BUCKET",
    "AYATI_ARCHIVE_LOCAL_ROOT",
    "AYATI_WORKER_CONTROL_TLS_KEY",
    "AYATI_WORKER_CONTROL_TLS_CERT",
    "AYATI_WORKER_CONTROL_LISTEN",
    "AYATI_CLOUD_WORKERS",
    "AYATI_AWS_TEST_ENDPOINT",
];

/// How an executor proves it may register with worker control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Identity {
    /// The operator's shared bootstrap key file (local and Compose).
    Bootstrap,
    /// Its ECS task identity, checked with STS and ECS (cloud workers).
    Aws,
}

#[derive(Clone)]
pub struct Executor {
    pub common: Common,
    pub worker_control: WorkerControl,
    pub identity: Identity,
    pub model: String,
    pub slots: usize,
    /// Plain Goose for tool-enabled runs; `prepared_goose` for ready slots.
    pub goose: GooseSettings,
    pub prepared_goose: PathBuf,
    pub trace_start: bool,
}

impl Executor {
    pub fn from_env() -> Result<Self> {
        Self::read(&mut Reader::process())
    }

    pub(crate) fn read(env: &mut Reader) -> Result<Self> {
        let common = Common::read(env);
        let identity = match env.text("AYATI_EXECUTOR_IDENTITY").as_deref() {
            None | Some("bootstrap") => Identity::Bootstrap,
            Some("aws") => Identity::Aws,
            Some(_) => {
                env.problem("AYATI_EXECUTOR_IDENTITY must be bootstrap or aws");
                Identity::Bootstrap
            }
        };
        let worker_control = WorkerControl::read(env, identity == Identity::Bootstrap);
        if worker_control.is_none() {
            env.problem(
                "Executors need AYATI_WORKER_CONTROL_URL and AYATI_WORKER_CONTROL_KEY_FILE",
            );
        }
        if worker_control
            .as_ref()
            .is_some_and(|c| c.https() && c.ca_file.is_none())
        {
            env.problem("An https:// worker control URL needs AYATI_WORKER_CONTROL_CA");
        }
        let model = env.required("AYATI_MODEL");
        let config = Self {
            common,
            identity,
            model: model.unwrap_or_default(),
            slots: env.number("AYATI_EXECUTOR_SLOTS", 2usize, 1..=16),
            goose: GooseSettings::read(env),
            prepared_goose: env
                .text("AYATI_PREPARED_GOOSE_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".cache/goose/v1.54.0-ayati.1/goose")),
            trace_start: env.flag("AYATI_TRACE_START"),
            worker_control: worker_control.unwrap_or(WorkerControl {
                url: String::new(),
                key_file: PathBuf::new(),
                ca_file: None,
            }),
        };
        env.refuse(CONTROLLER_ONLY, "Executors");
        env.finish("executor")?;
        Ok(config)
    }
}
