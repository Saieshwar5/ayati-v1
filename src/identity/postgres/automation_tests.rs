//! A pooled controller with one workspace, its owner, a conversation and a
//! managed agent: the setup automation, trigger and system tests share.
use super::{tests::*, *};
pub(super) async fn setup() -> (PgStore, User, String, String, String) {
    let mut db = fixture().await;
    db.pooled = true;
    let user = user(&db, "automation-author").await;
    let w = workspace(&db, &user).await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &user, &w).await;
    (db, user, w, chat, agent)
}
