use anyhow::{Context, Result, bail};
use ayati::config::Config;

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("control-mcp") {
        return ayati_guest::control_mcp::serve().await;
    }
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        return ayati::runtime::proof_tool::serve().await;
    }
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ayati=info".into()),
        )
        .init();
    let config = Config::from_env()?;
    match std::env::args().nth(1).as_deref().unwrap_or("help") {
        "workspace-db" => {
            ayati::identity::postgres::cli::run(
                &config.data_dir,
                &std::env::args().nth(2).unwrap_or_else(|| "check".into()),
            )
            .await
        }
        "simulate" => {
            ayati::identity::simulation::serve(
                &std::path::PathBuf::from(
                    std::env::args()
                        .nth(2)
                        .context("Specify a dedicated simulation data directory")?,
                ),
                config.port,
            )
            .await
        }
        "identity" => ayati::identity::serve(&config.data_dir, config.port).await,
        "worker-health" => ayati::identity::worker_health::check(),
        "executor" => {
            anyhow::ensure!(
                std::env::args().nth(2).is_none(),
                "Use executor without arguments"
            );
            ayati::identity::executor::serve(&config.data_dir).await
        }
        "api-health" => {
            reqwest::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(5))
                .build()?
                .get(format!("http://127.0.0.1:{}/health/ready", config.port))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
        "worker" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            match args.as_slice() {
                [] => ayati::identity::worker::serve(&config.data_dir).await,
                [flag] if flag == "--controller-assignment" => {
                    ayati::identity::controlled_worker::serve(&config.data_dir).await
                }
                [flag, id] if flag == "--assignment" => {
                    ayati::identity::assignment_worker::serve(&config.data_dir, id).await
                }
                _ => {
                    bail!("Use worker, worker --assignment <id>, or worker --controller-assignment")
                }
            }
        }
        "assignment-launcher" => {
            anyhow::ensure!(
                std::env::args().nth(2).is_none(),
                "Use assignment-launcher without arguments"
            );
            ayati::identity::assignment_launcher::serve(&config.data_dir).await
        }
        "help" | "--help" | "-h" => {
            println!(
                "Cuckoo — business agent workspace\n\nayati identity   Start the authenticated Cuckoo workspaces\nayati worker     Run an independent PostgreSQL polling worker\nayati worker --assignment <id>  Execute one reserved assignment and exit\nayati worker --controller-assignment  Execute a private stdin assignment through controller APIs\nayati assignment-launcher  Launch temporary local assignment workers\nayati executor   Run a shared pool of prepared Goose sessions\nayati api-health Check API readiness on its internal port\nayati worker-health Check this container worker heartbeat\nayati simulate <directory>  Start an isolated scripted company (requires simulation database)\nayati workspace-db <migrate|check|import>  Manage the PostgreSQL identity store\n\nConfiguration: .env (see .env.example)."
            );
            Ok(())
        }
        command => bail!("Unknown command {command}; use `ayati --help`"),
    }
}
