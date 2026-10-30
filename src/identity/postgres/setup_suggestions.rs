//! The "setup_suggestion" executor task purpose: AI suggestions for a saved
//! permission review. The draft starts with every action blocked; a finished
//! suggestion fills in reviewable choices only if the requester has not edited
//! the draft meanwhile. Suggestions never activate permissions.
use super::{executor_tasks::Task, *};
use crate::identity::{
    chat::execution::Prepared,
    control::setup_model::{Draft, Suggestion},
};

/// The draft still waits for this suggestion and its requester can still use it.
pub(super) async fn allowed(tx: &DatabaseTransaction, task: &Task, user: &User) -> Result<bool> {
    if super::agent_profiles::managed_access(tx, &user.id, &task.workspace, &task.agent, true)
        .await
        .is_err()
    {
        return Ok(false);
    }
    Ok(query(tx,"SELECT id FROM relay_control_drafts WHERE workspace=$1 AND agent=$2 AND user_id=$3 AND id=$4 AND state='draft' AND suggestion_context IS NOT NULL",
        vec![task.workspace.clone().into(),task.agent.clone().into(),user.id.clone().into(),task.id.clone().into()]).await?.is_some())
}

pub(super) async fn context(db: &impl ConnectionTrait, task: &Task) -> Result<Prepared> {
    let row = query(db,"SELECT suggestion_context FROM relay_control_drafts WHERE workspace=$1 AND id=$2 AND suggestion_context IS NOT NULL",
        vec![task.workspace.clone().into(),task.id.clone().into()]).await?.context(super::worker_control::ControlDenied)?;
    let prepared: Prepared = serde_json::from_value(row.try_get("", "suggestion_context")?)?;
    ensure!(
        prepared.version == 1
            && !prepared.control_tools
            && prepared.workspace == task.workspace
            && prepared.agent == task.agent
            && prepared.reply == task.id,
        "Unsupported setup suggestion context"
    );
    Ok(prepared)
}

/// Save the executor's result as reviewable choices, or explain why not.
pub(super) async fn finished(
    tx: &DatabaseTransaction,
    task: &Task,
    state: &str,
    text: &str,
) -> Result<()> {
    let Some(row) = query(tx,"SELECT data,revision,state FROM relay_control_drafts WHERE workspace=$1 AND id=$2 AND suggestion_context IS NOT NULL FOR UPDATE",
        vec![task.workspace.clone().into(),task.id.clone().into()]).await? else {
        return Ok(());
    };
    let mut draft: Draft = serde_json::from_value(row.try_get("", "data")?)?;
    let edited =
        row.try_get::<i64>("", "revision")? != 1 || row.try_get::<String>("", "state")? != "draft";
    let outcome = if edited {
        Err((
            "discarded",
            "AI suggestions arrived after you changed this review, so your choices were kept.",
        ))
    } else if state != "complete" {
        Err((
            "failed",
            "AI suggestions could not be prepared. Your active permissions have not changed; review manually or try again.",
        ))
    } else {
        let mut suggested = draft.clone();
        match crate::identity::control::setup_assistant::apply(&mut suggested, text) {
            Ok(()) => Ok(suggested),
            Err(_) => Err((
                "failed",
                "The assistant returned an unsupported draft. Your active permissions have not changed; review manually or try again.",
            )),
        }
    };
    draft = match outcome {
        Ok(mut suggested) => {
            suggested.suggestion = Some(Suggestion {
                state: "ready".into(),
                detail: "AI suggestions are ready for your review.".into(),
            });
            suggested
        }
        Err((state, detail)) => {
            draft.suggestion = Some(Suggestion {
                state: state.into(),
                detail: detail.into(),
            });
            draft
        }
    };
    save(tx, task, &draft, !edited).await
}

pub(super) async fn interrupted(tx: &DatabaseTransaction, task: &Task) -> Result<()> {
    stopped(
        tx,
        task,
        "AI suggestions stopped before finishing. Review manually or start a new review.",
    )
    .await
}

pub(super) async fn invalidate(tx: &DatabaseTransaction, task: &Task) -> Result<()> {
    stopped(
        tx,
        task,
        "AI suggestions stopped because access changed. Start a new review with current access.",
    )
    .await
}

async fn stopped(tx: &DatabaseTransaction, task: &Task, detail: &str) -> Result<()> {
    let Some(row) = query(tx,"SELECT data FROM relay_control_drafts WHERE workspace=$1 AND id=$2 AND suggestion_context IS NOT NULL FOR UPDATE",
        vec![task.workspace.clone().into(),task.id.clone().into()]).await? else {
        return Ok(());
    };
    let mut draft: Draft = serde_json::from_value(row.try_get("", "data")?)?;
    draft.suggestion = Some(Suggestion {
        state: "failed".into(),
        detail: detail.into(),
    });
    save(tx, task, &draft, true).await
}

/// Clear the pending context. A new revision makes an open review reload; an
/// edited draft keeps the requester's revision so their next save still applies.
async fn save(tx: &DatabaseTransaction, task: &Task, draft: &Draft, bump: bool) -> Result<()> {
    execute(tx,"UPDATE relay_control_drafts SET data=$3,suggestion_context=NULL,revision=revision+CASE WHEN $4 THEN 1 ELSE 0 END WHERE workspace=$1 AND id=$2",
        vec![task.workspace.clone().into(),task.id.clone().into(),serde_json::to_value(draft)?.into(),bump.into()]).await?;
    Ok(())
}
