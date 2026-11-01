//! `ayati identity` and `ayati simulate`: the only processes with database,
//! WorkOS, storage and connector access. Controllers prepare and record work
//! but never run Goose, so they hold no model key.
use super::{Common, Reader, WorkerControl, absolute};
use anyhow::Result;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

/// How agent replies run. Controllers always dispatch to pooled executors; only
/// the company simulation may answer with scripted replies inside the controller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Execution {
    Pooled,
    Scripted,
}

impl Execution {
    fn parse(value: Option<&str>, simulation: bool) -> std::result::Result<Self, &'static str> {
        match (value, simulation) {
            (None, false) | (Some("pooled"), _) => Ok(Self::Pooled),
            (None, true) | (Some("scripted"), true) => Ok(Self::Scripted),
            (Some("scripted"), false) => {
                Err("Scripted replies are only available in ayati simulate")
            }
            _ => Err("AYATI_CHAT_EXECUTION must be pooled (or scripted in a simulation)"),
        }
    }
}

#[derive(Clone)]
pub struct Controller {
    pub common: Common,
    /// Canonical browser origin: the HTTPS public origin, or local loopback HTTP.
    pub origin: String,
    pub bind: IpAddr,
    pub database_url: String,
    /// WorkOS client ID and API key; absent only in the scripted simulation.
    pub workos: Option<(String, String)>,
    pub execution: Execution,
    pub worker_control: Option<WorkerControl>,
    /// Where worker control listens; defaults to the URL's loopback address.
    pub worker_listen: Option<SocketAddr>,
    /// Certificate and private key for an `https://` worker control URL.
    pub worker_tls: Option<TlsFiles>,
    pub pool: Pool,
    /// Providers the executors are configured for (their keys stay on executors).
    pub worker_providers: Vec<String>,
    pub default_model: String,
    pub library: Library,
    pub archive: super::Archive,
    pub mcp_servers_file: Option<PathBuf>,
    /// Composio's project API key (a private 0600 file); no file, no Composio.
    pub composio_key_file: Option<PathBuf>,
    /// A loopback fixture for tests; the official API otherwise.
    pub composio_base_url: Option<String>,
    /// Toolkit to Composio auth config, for apps that sign in with your own OAuth app.
    pub composio_auth_configs: std::collections::BTreeMap<String, String>,
    pub event_sources_file: Option<PathBuf>,
    pub handoff_wait_seconds: i64,
    pub event_receipt_limit: i64,
    pub cloud: Option<CloudWorkers>,
    /// Days live-update notices are kept; older browser positions reload (D226).
    pub retain_sync_days: i64,
    /// Verified emails of Cuckoo platform administrators: they issue workspace
    /// invite codes and may create workspaces without one. Never set in the app.
    pub platform_admins: Vec<String>,
    /// Seconds between pool metric snapshots; 0 turns them off.
    pub metrics_seconds: u64,
    /// JSON logs (containers): metrics are CloudWatch embedded metric lines.
    pub log_json: bool,
    /// Loopback fake for STS and ECS in local tests; never a real endpoint.
    pub aws_test_endpoint: Option<String>,
    pub trace_start: bool,
    /// After a stop signal: seconds to keep serving while reporting not ready.
    pub drain_seconds: u64,
    /// After a stop signal: total seconds before unfinished work is abandoned.
    pub shutdown_seconds: u64,
}

#[derive(Clone)]
pub struct TlsFiles {
    pub certificate: PathBuf,
    pub key: PathBuf,
}

/// Executor pool admission, leases and per-assignment duration.
#[derive(Clone)]
pub struct Pool {
    pub lease_seconds: u64,
    pub run_seconds: u64,
    pub capacity: usize,
    pub chat_reserve: usize,
    pub workspace_limit: usize,
    pub model_limit: usize,
    pub expiry_seconds: u64,
}

/// Library file storage: private S3 in the cloud, an absolute directory locally.
#[derive(Clone, Default)]
pub struct Library {
    pub local_root: Option<PathBuf>,
    pub s3_bucket: Option<String>,
    pub s3_endpoint: Option<String>,
}

/// The ECS cluster, release and task role cloud executors must prove.
#[derive(Clone)]
pub struct CloudWorkers {
    pub cluster_arn: String,
    pub release_arn: String,
    pub role_arn: String,
}

impl Controller {
    pub fn from_env(simulation: bool) -> Result<Self> {
        Self::read(&mut Reader::process(), simulation)
    }

    pub(crate) fn read(env: &mut Reader, simulation: bool) -> Result<Self> {
        let common = Common::read(env);
        let (origin, bind) = origin(env, common.port);
        let hosted = origin.starts_with("https://");
        let database_url = if simulation {
            env.required("RELAY_SIMULATION_DATABASE_URL")
        } else {
            env.required("RELAY_DATABASE_URL")
        }
        .unwrap_or_default();
        let workos = if simulation {
            None
        } else {
            let key = env
                .text("WORKOS_API_KEY")
                .or_else(|| env.text("WORK_OS_API_KEY"));
            let client = env
                .text("WORKOS_CLIENT_ID")
                .or_else(|| env.text("WORK_OS_CLIENT_ID"));
            match (client, key) {
                (Some(client), Some(key)) if client.starts_with("client_") => Some((client, key)),
                (Some(_), Some(_)) => {
                    env.problem("WORKOS_CLIENT_ID must start with client_");
                    None
                }
                _ => {
                    env.problem("Set WORKOS_CLIENT_ID and WORKOS_API_KEY");
                    None
                }
            }
        };
        let execution =
            match Execution::parse(env.text("AYATI_CHAT_EXECUTION").as_deref(), simulation) {
                Ok(execution) => execution,
                Err(error) => {
                    env.problem(error);
                    Execution::Pooled
                }
            };
        let worker_control = WorkerControl::read(env, true);
        if execution == Execution::Pooled && worker_control.is_none() {
            env.problem(
                "Pooled execution needs AYATI_WORKER_CONTROL_URL and AYATI_WORKER_CONTROL_KEY_FILE",
            );
        }
        let (worker_listen, worker_tls) = worker_listener(env, worker_control.as_ref());
        let pool = Pool::read(env);
        let worker_providers: Vec<String> = env
            .text("AYATI_WORKER_PROVIDERS")
            .map(|v| v.split(',').map(|p| p.trim().to_owned()).collect())
            .unwrap_or_default();
        if !worker_providers
            .iter()
            .all(|p| matches!(p.as_str(), "openai" | "fireworks-ai"))
        {
            env.problem(
                "AYATI_WORKER_PROVIDERS must be a comma-separated list of openai,fireworks-ai",
            );
        }
        let library = Library::read(env);
        if hosted && library.local_root.is_some() {
            env.problem("Hosted files require private S3 storage, not CUCKOO_LIBRARY_LOCAL_ROOT");
        }
        let config = Self {
            origin,
            bind,
            database_url,
            workos,
            execution,
            worker_control,
            worker_listen,
            worker_tls,
            pool,
            worker_providers,
            default_model: env
                .text("AYATI_MODEL")
                .unwrap_or_else(|| "accounts/fireworks/models/glm-5p3-flash".into()),
            library,
            archive: super::Archive::read(env, hosted),
            mcp_servers_file: absolute(env, "AYATI_MCP_SERVERS_FILE"),
            composio_key_file: absolute(env, "AYATI_COMPOSIO_KEY_FILE"),
            composio_base_url: env.text("AYATI_COMPOSIO_BASE_URL"),
            composio_auth_configs: composio_auth_configs(env),
            event_sources_file: absolute(env, "AYATI_EVENT_SOURCES_FILE"),
            handoff_wait_seconds: env.number("AYATI_HANDOFF_WAIT_SECONDS", 604_800, 60..=2_592_000),
            event_receipt_limit: env.number("AYATI_EVENT_RECEIPT_LIMIT", 100_000, 1..=1_000_000),
            aws_test_endpoint: test_endpoint(env),
            metrics_seconds: env.number("AYATI_METRICS_SECONDS", 60, 0..=3600),
            retain_sync_days: env.number("AYATI_RETAIN_SYNC_DAYS", 30, 1..=3650),
            platform_admins: platform_admins(env),
            log_json: env.text("AYATI_LOG_FORMAT").as_deref() == Some("json"),
            cloud: CloudWorkers::read(env),
            trace_start: env.flag("AYATI_TRACE_START"),
            drain_seconds: env.number("AYATI_DRAIN_SECONDS", 0, 0..=300),
            shutdown_seconds: env.number("AYATI_SHUTDOWN_SECONDS", 60, 1..=600),
            common,
        };
        if config.drain_seconds >= config.shutdown_seconds {
            env.problem("AYATI_DRAIN_SECONDS must be smaller than AYATI_SHUTDOWN_SECONDS");
        }
        env.refuse(&["AYATI_EXECUTOR_SLOTS"], "The controller");
        // Model keys belong on executors (.env.executor locally).
        env.refuse(&["OPENAI_API_KEY", "FIREWORKS_API_KEY"], "The controller");
        env.finish("controller")?;
        Ok(config)
    }

    pub fn hosted(&self) -> bool {
        self.origin.starts_with("https://")
    }
}

impl Pool {
    fn read(env: &mut Reader) -> Self {
        let capacity = env.number("AYATI_EXECUTOR_CAPACITY", 2usize, 1..=1024);
        let pool = Self {
            lease_seconds: env.number("AYATI_ASSIGNMENT_LEASE_SECONDS", 120, 6..=300),
            run_seconds: env.number("AYATI_ASSIGNMENT_RUN_SECONDS", 180, 1..=86_400),
            capacity,
            chat_reserve: env.number("AYATI_EXECUTOR_CHAT_RESERVE", 0usize, 0..=1023),
            workspace_limit: env.number("AYATI_EXECUTOR_WORKSPACE_LIMIT", capacity, 1..=1024),
            model_limit: env.number("AYATI_EXECUTOR_MODEL_LIMIT", capacity, 1..=1024),
            expiry_seconds: env.number("AYATI_EXECUTOR_EXPIRY_SECONDS", 15, 6..=120),
        };
        if pool.chat_reserve
            >= pool
                .capacity
                .min(pool.workspace_limit)
                .min(pool.model_limit)
        {
            env.problem(
                "AYATI_EXECUTOR_CHAT_RESERVE must be smaller than global, workspace and model limits",
            );
        }
        pool
    }
}

impl Library {
    fn read(env: &mut Reader) -> Self {
        let library = Self {
            local_root: env.text("CUCKOO_LIBRARY_LOCAL_ROOT").map(PathBuf::from),
            s3_bucket: env.text("CUCKOO_LIBRARY_S3_BUCKET"),
            s3_endpoint: env.text("CUCKOO_LIBRARY_S3_ENDPOINT"),
        };
        if library.local_root.is_some() && library.s3_bucket.is_some() {
            env.problem("Configure only one Library storage backend");
        }
        if library
            .local_root
            .as_ref()
            .is_some_and(|root| !root.is_absolute())
        {
            env.problem("CUCKOO_LIBRARY_LOCAL_ROOT must be an absolute development storage path");
        }
        library
    }
}

impl CloudWorkers {
    fn read(env: &mut Reader) -> Option<Self> {
        if !env.flag("AYATI_CLOUD_WORKERS") {
            return None;
        }
        let cluster_arn = env.required("AYATI_AWS_EXECUTOR_CLUSTER_ARN");
        let release_arn = env.required("AYATI_AWS_EXECUTOR_RELEASE_ARN");
        let role_arn = env.required("AYATI_AWS_EXECUTOR_ROLE_ARN");
        Some(Self {
            cluster_arn: cluster_arn?,
            release_arn: release_arn?,
            role_arn: role_arn?,
        })
    }
}

/// `gmail=ac_1,googlecalendar=ac_2`: which Composio auth config each toolkit uses.
fn composio_auth_configs(env: &mut Reader) -> std::collections::BTreeMap<String, String> {
    let mut configs = std::collections::BTreeMap::new();
    for pair in env
        .text("AYATI_COMPOSIO_AUTH_CONFIGS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        match pair.split_once('=') {
            Some((toolkit, config)) if !toolkit.trim().is_empty() && !config.trim().is_empty() => {
                configs.insert(toolkit.trim().to_lowercase(), config.trim().to_owned());
            }
            _ => env.problem(
                "AYATI_COMPOSIO_AUTH_CONFIGS must look like gmail=ac_123,googlecalendar=ac_456",
            ),
        }
    }
    configs
}

fn platform_admins(env: &mut Reader) -> Vec<String> {
    let admins: Vec<String> = env
        .text("AYATI_PLATFORM_ADMINS")
        .map(|value| {
            value
                .split(',')
                .map(|e| e.trim().to_lowercase())
                .filter(|e| !e.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if admins
        .iter()
        .any(|e| !e.contains('@') || e.chars().any(char::is_whitespace))
    {
        env.problem("AYATI_PLATFORM_ADMINS must be a comma-separated list of email addresses");
    }
    admins
}

fn test_endpoint(env: &mut Reader) -> Option<String> {
    let value = env.text("AYATI_AWS_TEST_ENDPOINT")?;
    let loopback = reqwest::Url::parse(&value).is_ok_and(|url| {
        url.scheme() == "http"
            && url
                .host_str()
                .and_then(|host| host.parse::<IpAddr>().ok())
                .is_some_and(|ip| ip.is_loopback())
    });
    if !loopback {
        env.problem("AYATI_AWS_TEST_ENDPOINT must be a loopback http:// URL");
    }
    if !env.present("AYATI_CLOUD_WORKERS") {
        env.problem("AYATI_AWS_TEST_ENDPOINT needs AYATI_CLOUD_WORKERS=1");
    }
    Some(value)
}

/// Worker control serves TLS exactly when its URL is `https://`; executors on
/// other machines then reach it through a private listen address.
fn worker_listener(
    env: &mut Reader,
    control: Option<&WorkerControl>,
) -> (Option<SocketAddr>, Option<TlsFiles>) {
    let listen = env.text("AYATI_WORKER_CONTROL_LISTEN").and_then(|value| {
        let parsed = value.parse::<SocketAddr>();
        if parsed.is_err() {
            env.problem("AYATI_WORKER_CONTROL_LISTEN must be an IP:port address");
        }
        parsed.ok()
    });
    let tls = match (
        absolute(env, "AYATI_WORKER_CONTROL_TLS_CERT"),
        absolute(env, "AYATI_WORKER_CONTROL_TLS_KEY"),
    ) {
        (Some(certificate), Some(key)) => Some(TlsFiles { certificate, key }),
        (None, None) => None,
        _ => {
            env.problem("Set both AYATI_WORKER_CONTROL_TLS_CERT and AYATI_WORKER_CONTROL_TLS_KEY");
            None
        }
    };
    let https = control.is_some_and(WorkerControl::https);
    if https != tls.is_some() {
        env.problem("Worker control uses TLS exactly when AYATI_WORKER_CONTROL_URL is https://");
    }
    if listen.is_some_and(|address| !address.ip().is_loopback()) && !https {
        env.problem("Worker control needs TLS to listen beyond loopback");
    }
    if control.is_some_and(|c| c.ca_file.is_some()) {
        env.problem("AYATI_WORKER_CONTROL_CA belongs to executors");
    }
    (listen, tls)
}

/// HTTPS public origin (behind a TLS proxy) or loopback-only local HTTP.
fn origin(env: &mut Reader, port: u16) -> (String, IpAddr) {
    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let bind = match env.text("AYATI_BIND_ADDRESS").map(|v| v.parse::<IpAddr>()) {
        None => local,
        Some(Ok(bind)) => bind,
        Some(Err(_)) => {
            env.problem("AYATI_BIND_ADDRESS must be an IP address");
            local
        }
    };
    let Some(origin) = env.text("AYATI_PUBLIC_ORIGIN") else {
        if bind != local {
            env.problem("A non-default bind address requires an HTTPS AYATI_PUBLIC_ORIGIN");
        }
        return (format!("http://127.0.0.1:{port}"), bind);
    };
    match reqwest::Url::parse(&origin) {
        Ok(url)
            if url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none() =>
        {
            (url.origin().ascii_serialization(), bind)
        }
        _ => {
            env.problem(
                "AYATI_PUBLIC_ORIGIN must be an HTTPS origin without credentials, path or query",
            );
            (format!("http://127.0.0.1:{port}"), bind)
        }
    }
}

#[cfg(test)]
impl Controller {
    /// Local defaults for HTTP tests that build an `App` directly.
    pub(crate) fn for_tests(origin: &str) -> Self {
        let mut env = Reader::from_pairs(&[
            (
                "RELAY_SIMULATION_DATABASE_URL",
                "postgresql://localhost/relay_simulation_test",
            ),
            ("AYATI_CHAT_EXECUTION", "scripted"),
        ]);
        let mut config = Self::read(&mut env, true).unwrap();
        config.origin = origin.into();
        config
    }
}

#[cfg(test)]
mod tests;
