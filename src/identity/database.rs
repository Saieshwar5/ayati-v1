//! Transitional adapter: existing local SQLite or explicitly configured PostgreSQL.
use super::{
    chat::store::{Job, NewChat, NewMessage},
    postgres::PgStore,
    store::{Store, User},
};
use anyhow::Result;
use serde_json::Value;
pub enum Database {
    Sqlite(Store),
    Postgres(PgStore),
}
impl From<Store> for Database {
    fn from(value: Store) -> Self {
        Self::Sqlite(value)
    }
}
impl Database {
    #[cfg(test)]
    pub fn sqlite(&self) -> &Store {
        match self {
            Self::Sqlite(db) => db,
            _ => panic!("SQLite test fixture required"),
        }
    }
    pub async fn login(&self, provider: &str, email: &str, hash: &str, sid: &str) -> Result<User> {
        match self {
            Self::Sqlite(db) => db.login(provider, email, hash, sid),
            Self::Postgres(db) => db.login(provider, email, hash, sid).await,
        }
    }
    pub async fn session(&self, hash: &str) -> Result<Option<(User, String, String)>> {
        match self {
            Self::Sqlite(db) => db.session(hash),
            Self::Postgres(db) => db.session(hash).await,
        }
    }
    pub async fn logout(&self, hash: &str) -> Result<()> {
        match self {
            Self::Sqlite(db) => db.logout(hash),
            Self::Postgres(db) => db.logout(hash).await,
        }
    }
    pub async fn update_profile(&self, user: &User, name: &str) -> Result<User> {
        match self {
            Self::Sqlite(db) => db.update_profile(user, name),
            Self::Postgres(db) => db.update_profile(user, name).await,
        }
    }
    pub async fn snapshot(&self, user: &User) -> Result<Value> {
        match self {
            Self::Sqlite(db) => db.snapshot(user),
            Self::Postgres(db) => db.snapshot(user).await,
        }
    }
    pub async fn create(&self, user: &User, name: &str, id: &str) -> Result<String> {
        match self {
            Self::Sqlite(db) => db.create(user, name, id),
            Self::Postgres(db) => db.create(user, name, id).await,
        }
    }
    pub async fn people(&self, user: &User, workspace: &str) -> Result<Value> {
        match self {
            Self::Sqlite(db) => db.people(user, workspace),
            Self::Postgres(db) => db.people(user, workspace).await,
        }
    }
    pub async fn reserve_invite(
        &self,
        user: &User,
        workspace: &str,
        email: &str,
        assigned: &str,
    ) -> Result<String> {
        match self {
            Self::Sqlite(db) => db.reserve_invite(user, workspace, email, assigned),
            Self::Postgres(db) => db.reserve_invite(user, workspace, email, assigned).await,
        }
    }
    pub async fn delivered(&self, id: &str, provider: Option<&str>) -> Result<()> {
        match self {
            Self::Sqlite(db) => db.delivered(id, provider),
            Self::Postgres(db) => db.delivered(id, provider).await,
        }
    }
    pub async fn accept(&self, user: &User, id: &str) -> Result<String> {
        match self {
            Self::Sqlite(db) => db.accept(user, id),
            Self::Postgres(db) => db.accept(user, id).await,
        }
    }
    pub async fn cancel(&self, user: &User, workspace: &str, id: &str) -> Result<Option<String>> {
        match self {
            Self::Sqlite(db) => db.cancel(user, workspace, id),
            Self::Postgres(db) => db.cancel(user, workspace, id).await,
        }
    }
    pub async fn change(
        &self,
        user: &User,
        workspace: &str,
        target: &str,
        assigned: Option<&str>,
    ) -> Result<()> {
        match self {
            Self::Sqlite(db) => db.change(user, workspace, target, assigned),
            Self::Postgres(db) => db.change(user, workspace, target, assigned).await,
        }
    }
    pub async fn role(&self, workspace: &str, actor: &str) -> Result<String> {
        match self {
            Self::Sqlite(db) => db.role(workspace, actor),
            Self::Postgres(db) => db.role(workspace, actor).await,
        }
    }
    pub async fn agent_access(&self, user: &str, workspace: &str, agent: &str) -> Result<bool> {
        match self {
            Self::Sqlite(db) => db.agent_access(user, workspace, agent),
            Self::Postgres(db) => db.agent_access(user, workspace, agent).await,
        }
    }
    pub async fn directory(&self, user: &User, workspace: &str) -> Result<Value> {
        match self {
            Self::Sqlite(db) => db.directory(user, workspace),
            Self::Postgres(db) => db.directory(user, workspace).await,
        }
    }
    pub async fn chat_grant(
        &self,
        user: &User,
        workspace: &str,
        target: &str,
        agent: &str,
        allowed: bool,
    ) -> Result<()> {
        match self {
            Self::Sqlite(db) => db.chat_grant(user, workspace, target, agent, allowed),
            Self::Postgres(db) => db.chat_grant(user, workspace, target, agent, allowed).await,
        }
    }
    pub async fn chat_create(
        &self,
        user: &User,
        workspace: &str,
        input: &NewChat,
    ) -> Result<String> {
        match self {
            Self::Sqlite(db) => db.chat_create(user, workspace, input),
            Self::Postgres(db) => db.chat_create(user, workspace, input).await,
        }
    }
    pub async fn chats(&self, user: &User, workspace: &str) -> Result<Value> {
        match self {
            Self::Sqlite(db) => db.chats(user, workspace),
            Self::Postgres(db) => db.chats(user, workspace).await,
        }
    }
    pub async fn chat_messages(&self, user: &User, workspace: &str, chat: &str) -> Result<Value> {
        match self {
            Self::Sqlite(db) => db.chat_messages(user, workspace, chat),
            Self::Postgres(db) => db.chat_messages(user, workspace, chat).await,
        }
    }
    pub async fn chat_send(
        &self,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
    ) -> Result<Option<Job>> {
        match self {
            Self::Sqlite(db) => db.chat_send(user, workspace, chat, input),
            Self::Postgres(db) => db.chat_send(user, workspace, chat, input).await,
        }
    }
    pub async fn chat_job_allowed(&self, job: &Job) -> bool {
        match self {
            Self::Sqlite(db) => db.chat_job_allowed(job),
            Self::Postgres(db) => db.chat_job_allowed(job).await,
        }
    }
    pub async fn chat_finish(&self, job: &Job, content: &str, state: &str) -> Result<()> {
        match self {
            Self::Sqlite(db) => db.chat_finish(job, content, state),
            Self::Postgres(db) => db.chat_finish(job, content, state).await,
        }
    }
}
