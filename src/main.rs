use anyhow::{Context, Result, bail};
use ayati::config;

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("control-mcp") {
        return ayati_guest::control_mcp::serve().await;
    }
    let command = std::env::args().nth(1).unwrap_or_else(|| "help".into());
    config::load_dotenv(&command);
    let logging = config::Logging::from_env()?;
    init_logging(&logging);
    // Health probes run every few seconds; only long-running roles announce themselves.
    let role = match command.as_str() {
        "identity" | "simulate" => Some("controller"),
        "executor" => Some("executor"),
        "workspace-db" => Some("migration"),
        _ => None,
    };
    if let Some(role) = role {
        let release = logging.release.as_deref().unwrap_or("unknown");
        tracing::info!(role, command, release, "Starting");
    }
    let result = run(&command).await;
    if let (true, Some(role), Err(error)) = (logging.json, role, &result) {
        // A JSON collector would split the default multi-line error output.
        // Errors name settings and stages, never secret values or message contents.
        tracing::error!(role, error = format!("{error:#}"), "Stopped with an error");
        std::process::exit(1);
    }
    result
}

fn init_logging(logging: &config::Logging) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "ayati=info".into());
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if logging.json {
        builder.json().flatten_event(true).init();
    } else {
        builder.init();
    }
}

async fn run(command: &str) -> Result<()> {
    match command {
        "workspace-db" => {
            ayati::identity::postgres::cli::run(
                &config::migration_database_url()?,
                &std::env::args().nth(2).unwrap_or_else(|| "check".into()),
            )
            .await
        }
        "simulate" => {
            let root = std::env::args()
                .nth(2)
                .context("Specify a dedicated simulation data directory")?;
            ayati::identity::simulation::serve(
                std::path::Path::new(&root),
                config::Controller::from_env(true)?,
            )
            .await
        }
        "identity" => ayati::identity::serve(config::Controller::from_env(false)?).await,
        "executor" => {
            anyhow::ensure!(
                std::env::args().nth(2).is_none(),
                "Use executor without arguments"
            );
            ayati::identity::executor::serve(config::Executor::from_env()?).await
        }
        "executor-health" => {
            ayati::identity::executor::health::check(&config::Common::from_env()?.data_dir)
        }
        "api-health" => {
            reqwest::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(5))
                .build()?
                .get(format!(
                    "http://127.0.0.1:{}/health/ready",
                    config::Common::from_env()?.port
                ))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!(
                "Cuckoo — business agent workspace\n\nayati identity   Start the controller: sign-in, workspaces, Chat and worker control\nayati executor   Run a shared pool of prepared Goose sessions for the controller\nayati api-health Check API readiness on its internal port\nayati executor-health Check that the local executor reports usable slots\nayati simulate <directory>  Start an isolated scripted company (requires simulation database)\nayati workspace-db <migrate|check|problem-reports>  Manage the PostgreSQL schema; list reported problems\n\nConfiguration: .env for the controller, .env.executor for executors (see .env.example and .env.executor.example)."
            );
            Ok(())
        }
        command => bail!("Unknown command {command}; use `ayati --help`"),
    }
}
