use super::super::*;
use postgres::RehearsalClaim;
use tokio_util::sync::CancellationToken;

pub(super) fn configured(app: &App, profile: &serde_json::Value) -> Result<bool> {
    let settings: agents::config::Settings = serde_json::from_value(profile["settings"].clone())?;
    let mut config = crate::runtime::config::GooseConfig::new(
        &std::env::temp_dir(),
        profile["model"].as_str().context("Missing model")?,
        &app.config.goose,
    );
    if !settings.provider.is_empty() {
        config.provider = settings.provider;
    }
    Ok(config.configured())
}
pub(crate) async fn start(app: Shared) {
    let owner = app.clone();
    app.runtimes.rehearsal_tasks.lock().await.spawn(async move {
        run_loop(owner).await;
    });
}
async fn run_loop(app: Shared) {
    let pg = &app.store;
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        if app.runtimes.stopping.is_cancelled() {
            break;
        }
        if pg.recover_rehearsals().await.is_ok() {
            while tasks.len() < 2 {
                match pg.claim_rehearsal().await {
                    Ok(Some(claim)) => {
                        tasks.spawn(run_one(
                            pg.clone(),
                            app.config.goose.clone(),
                            claim,
                            app.runtimes.stopping.child_token(),
                        ));
                    }
                    Ok(None) => break,
                    Err(_) => {
                        tracing::warn!("Sample queue unavailable; saved requests remain pending");
                        break;
                    }
                }
            }
        }
        tokio::select! {
            _=app.runtimes.stopping.cancelled()=>break,
            _=app.runtimes.rehearsal_wake.notified()=>{},
            _=tokio::time::sleep(std::time::Duration::from_secs(2))=>{},
            _=tasks.join_next(),if !tasks.is_empty()=>{},
        }
    }
    while tasks.join_next().await.is_some() {}
}
async fn run_one(
    pg: postgres::PgStore,
    goose: crate::config::GooseSettings,
    claim: RehearsalClaim,
    stopping: CancellationToken,
) {
    let root = std::env::temp_dir().join(format!("ayati-agent-sample-{}", uuid::Uuid::new_v4()));
    let cancelled = stopping.child_token();
    let result = async {
        tokio::fs::create_dir_all(&root).await?;
        tokio::fs::write(root.join(".env"), "").await?;
        // runtime_config supplies an explicit empty Goose extension list. No
        // business broker/owner extension can be obtained from this record.
        let work =
            chat::execution::execute_prepared(&claim.prepared, &root, &cancelled, None, &goose);
        tokio::pin!(work);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                result=&mut work=>break result,
                _=stopping.cancelled()=>{ cancelled.cancel();break work.await; },
                _=heartbeat.tick()=>{
                    if !pg.heartbeat_rehearsal(&claim).await.unwrap_or(false) {
                        cancelled.cancel();break work.await;
                    }
                }
            }
        }
    }
    .await;
    let output = result
        .ok()
        .map(|text| format!("Draft-only sample — no connected actions were executed.\n\n{text}"))
        .filter(|text| text.len() <= 65536);
    if pg
        .finish_rehearsal(&claim, output.as_deref(), stopping.is_cancelled())
        .await
        .is_err()
    {
        tracing::warn!(
            "Sample result could not be confirmed; its saved lease will recover without automatic inference replay"
        );
    }
    chat::execution::cleanup(&root).await;
}
