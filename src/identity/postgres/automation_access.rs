//! Who may set up an event trigger and where it may run.
use super::*;
/// What an automation needs from its person to run: they may still look after
/// the agent and use it directly, and, once it names its conversation, ask the
/// agent there. A run happens in that conversation as its person (schema 151).
pub(super) async fn run_authority(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    runs_in: Option<&str>,
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
    match runs_in {
        Some(chat) => may_ask_in(tx, user, w, agent, chat).await,
        None => Ok(()),
    }
}
/// `user` may ask `agent` in `chat`: the checks the ordinary intake makes for a
/// person's message there (conversation access, the agent in it and allowed for
/// them, announcements, a conversation still accepting messages).
pub(super) async fn may_ask_in(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    chat: &str,
) -> Result<()> {
    super::chat::access(tx, user, w, chat).await?;
    ensure!(
        super::chat::agent_access_in(tx, &user.id, w, agent, chat).await?,
        "Current agent request permission is required"
    );
    ensure!(query(tx,"SELECT a.agent FROM relay_chat_agents a JOIN relay_chats c ON c.workspace=a.workspace AND c.id=a.chat WHERE a.workspace=$1 AND a.chat=$2 AND a.agent=$3 AND (NOT c.announcements OR EXISTS(SELECT 1 FROM relay_members m WHERE m.workspace=$1 AND m.user_id=$4 AND m.role='Admin'))",vec![w.into(),chat.into(),agent.into(),user.id.clone().into()]).await?.is_some(),"The agent must be in that conversation, and the person allowed to ask it there");
    super::channel_lifecycle::accepting(tx, w, chat).await
}
