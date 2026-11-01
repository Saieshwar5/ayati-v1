//! The controller runs a panel update itself when the panel has saved bindings
//! (people-and-agents-first, Phase 3 HP3): the reviewed reads go through the
//! ordinary action path (same rules, receipts and Activity record), the numbers are
//! filled from the bindings, and the update publishes with no model call. When a
//! number moves meaningfully, or anything doesn't fit, the same update goes to the
//! agent, who writes what changed.
use crate::identity::{
    Shared, chat::model::Job, control::model::ActionRequest, home_panels::maintenance::Refresh,
    postgres::PgStore,
};
use crate::panels::bindings;
use anyhow::{Context, Result, ensure};

/// A number moving by more than this share is worth the agent's note.
const MEANINGFUL: f64 = 0.10;

/// Start every update claimed for the controller. Run by the scheduler loop.
pub(crate) async fn run(app: &Shared) -> Result<usize> {
    let jobs = app.store.direct_pick().await?;
    let started = jobs.len();
    for job in jobs {
        let owner = app.clone();
        tokio::spawn(async move {
            if let Err(error) = refresh(&owner.store, &job).await {
                let why = format!("{error:#}");
                if owner
                    .store
                    .direct_release(&job, &why)
                    .await
                    .unwrap_or(false)
                {
                    owner.runtimes.chat_wake.notify_one();
                }
            }
        });
    }
    Ok(started)
}

pub(crate) async fn refresh(pg: &PgStore, job: &Job) -> Result<()> {
    let plan = pg.direct_plan(job).await?;
    for scope in &plan.scopes {
        let request = ActionRequest {
            connection: scope["connection"]
                .as_str()
                .context("Read connection")?
                .into(),
            operation: scope["operation"]
                .as_str()
                .context("Read operation")?
                .into(),
            arguments: scope["arguments"].clone(),
        };
        let proposed = pg.control_propose(job, &request).await?;
        crate::identity::control::dispatch(pg, &job.workspace, proposed).await?;
    }
    let results = pg.direct_results(job, &plan.scopes).await?;
    ensure!(
        results.iter().all(|r| !r.is_null()),
        "A source read did not complete"
    );
    let sections = bindings::apply_all(&plan.sections, &plan.bindings, &results)?;
    ensure!(
        !bindings::meaningful_all(&plan.sections, &sections, &plan.bindings, MEANINGFUL),
        "Something meaningful changed: the agent writes what changed"
    );
    let input = Refresh {
        sections: sections
            .into_iter()
            .map(serde_json::from_value)
            .collect::<std::result::Result<_, _>>()?,
        what_changed: None,
        bindings: None,
    };
    pg.refresh_home_panel(job, &input).await?;
    pg.direct_done(job).await?;
    pg.chat_finish(job, "Numbers refreshed from the source.", "complete")
        .await?;
    Ok(())
}
