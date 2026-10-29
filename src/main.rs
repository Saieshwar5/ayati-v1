use anyhow::{Context, Result, bail};
use ayati::config;

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("control-mcp") {
        return ayati_guest::control_mcp::serve().await;
    }
    let command = std::env::args().nth(1).unwrap_or_else(|| "help".into());
    config::load_dotenv(&command);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ayati=info".into()),
        )
        .init();
    match command.as_str() {
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
                "Cuckoo — business agent workspace\n\nayati identity   Start the controller: sign-in, workspaces, Chat and worker control\nayati executor   Run a shared pool of prepared Goose sessions for the controller\nayati api-health Check API readiness on its internal port\nayati simulate <directory>  Start an isolated scripted company (requires simulation database)\nayati workspace-db <migrate|check>  Manage the PostgreSQL schema\n\nConfiguration: .env for the controller, .env.executor for executors (see .env.example and .env.executor.example)."
            );
            Ok(())
        }
        command => bail!("Unknown command {command}; use `ayati --help`"),
    }
}
