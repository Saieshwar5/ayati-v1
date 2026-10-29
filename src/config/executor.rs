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
    "AYATI_EVENT_SOURCES_FILE",
    "CUCKOO_LIBRARY_S3_BUCKET",
    "CUCKOO_LIBRARY_LOCAL_ROOT",
];

#[derive(Clone)]
pub struct Executor {
    pub common: Common,
    pub worker_control: WorkerControl,
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
        let worker_control = WorkerControl::read(env);
        if worker_control.is_none() {
            env.problem(
                "Executors need AYATI_WORKER_CONTROL_URL and AYATI_WORKER_CONTROL_KEY_FILE",
            );
        }
        let model = env.required("AYATI_MODEL");
        let config = Self {
            common,
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
            }),
        };
        env.refuse(CONTROLLER_ONLY, "Executors");
        env.finish("executor")?;
        Ok(config)
    }
}
