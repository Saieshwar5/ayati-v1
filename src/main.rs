use anyhow::{Context, Result, bail};
use ayati::{
    agent::Agent,
    config::Config,
    files::Files,
    model::CompatibleModel,
    server::{WebState, router},
    storage::Store,
};
use std::{os::unix::fs::PermissionsExt, sync::Arc};
use tokio_util::sync::CancellationToken;

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ayati=info".into()),
        )
        .init();
    let config = Config::from_env()?;
    match std::env::args().nth(1).as_deref().unwrap_or("serve") {
        "serve" | "start" => serve(config).await,
        "status" => {
            let response = reqwest::get(format!("http://127.0.0.1:{}/api/health", config.port))
                .await
                .context("Ayati is not reachable; start it with `ayati serve`")?;
            println!("{}", response.error_for_status()?.text().await?);
            Ok(())
        }
        "stop" => {
            reqwest::Client::new()
                .post(format!("http://127.0.0.1:{}/api/shutdown", config.port))
                .send()
                .await
                .context("Ayati is not reachable")?
                .error_for_status()?;
            println!("Ayati is stopping.");
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!(
                "Ayati — local personal assistant\n\nayati serve   Start the local daemon\nayati status  Check the running daemon\nayati stop    Stop the daemon and its current work\n\nConfiguration: .env (see .env.example). Web: http://127.0.0.1:8765"
            );
            Ok(())
        }
        command => bail!("Unknown command {command}; use `ayati --help`"),
    }
}

async fn serve(config: Config) -> Result<()> {
    // Bind before opening the database: a second daemon must not interrupt the first one's tasks.
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, config.port))
        .await
        .context("Could not bind the local port; another Ayati instance may be running")?;
    std::fs::create_dir_all(&config.data_dir)?;
    std::fs::set_permissions(&config.data_dir, std::fs::Permissions::from_mode(0o700))?;
    let data = config.data_dir.canonicalize()?;
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("daemon.lock"))?;
    lock_file
        .try_lock()
        .context("Another Ayati daemon is using this data directory")?;
    let store = Arc::new(Store::open(&data.join("ayati.sqlite"))?);
    let files = Arc::new(Files::new(&data, store.clone())?);
    let model = Arc::new(CompatibleModel::new(
        &config.model_base_url,
        config.api_key.clone(),
        config.model.clone(),
    )?);
    let agent = Agent::start(store, files, model, config.context_bytes);
    let shutdown = CancellationToken::new();
    let state = WebState {
        agent: agent.clone(),
        model_ready: config.api_key.is_some(),
        model_name: config.model,
        port: config.port,
        shutdown: shutdown.clone(),
    };
    tracing::info!(
        port = config.port,
        "Ayati ready at http://127.0.0.1:{}",
        config.port
    );
    if !state.model_ready {
        tracing::info!("Model not configured; add FIREWORKS_API_KEY to .env and restart");
    }
    let signal = shutdown.clone();
    tokio::spawn(async move {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
        signal.cancel();
    });
    let stopping_agent = agent.clone();
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async move {
            shutdown.cancelled().await;
            agent.stop_all();
        })
        .await?;
    stopping_agent.wait_until_idle().await;
    Ok(())
}
