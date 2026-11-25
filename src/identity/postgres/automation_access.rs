//! Who may run an automation and post its results (event triggers, task posts).
use super::*;
/// What an automation needs from its person to run: they may still look after
/// the agent and use it directly, and post where it posts, if anywhere. Runs
/// happen in its own work room (schema 56), not in a conversation.
pub(super) async fn run_authority(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    post_to: Option<&str>,
) -> Result<()> {
    let profile = super::agent_profiles::managed_access(tx, &user.id, w, agent, true).await?;
    ensure!(
        profile["revision"].as_i64().is_some_and(|r| r > 0),
        "Use a managed agent for shared routines"
    );
    ensure!(
        super::chat::agent_access(tx, &user.id, w, agent).await?,
        "Current agent request permission is required"
    );
    match post_to {
        Some(chat) => authority_for(tx, user, w, agent, true, chat).await,
        None => Ok(()),
    }
}
pub(super) async fn authority(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    chat: &str,
) -> Result<()> {
    authority_for(tx, user, w, agent, true, chat).await
}
/// May post the agent's work in `chat` as `user`. `manage`: `user` must look after
/// the agent (an event trigger they set up), not only be allowed to ask it (a task
/// they asked for).
pub(super) async fn authority_for(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    manage: bool,
    chat: &str,
) -> Result<()> {
    let profile = super::agent_profiles::managed_access(tx, &user.id, w, agent, manage).await?;
    ensure!(
        profile["revision"].as_i64().is_some_and(|r| r > 0),
        "Use a managed agent for shared routines"
    );
    super::chat::access(tx, user, w, chat).await?;
    ensure!(
        super::chat::agent_access_in(tx, &user.id, w, agent, chat).await?,
        "Current agent request permission is required"
    );
    ensure!(query(tx,"SELECT a.agent FROM relay_chat_agents a JOIN relay_chats c ON c.workspace=a.workspace AND c.id=a.chat WHERE a.workspace=$1 AND a.chat=$2 AND a.agent=$3 AND (NOT c.announcements OR EXISTS(SELECT 1 FROM relay_members m WHERE m.workspace=$1 AND m.user_id=$4 AND m.role='Admin'))",vec![w.into(),chat.into(),agent.into(),user.id.clone().into()]).await?.is_some(),"Agent participation and destination posting authority are required");
    super::channel_lifecycle::accepting(tx, w, chat).await
}
