//! Handing a chat over from a message (schema 136).
//!
//! The member picked a message the other side sent and said in one instruction
//! what their assistant should do. That way of starting was replaced by one
//! position per chat (`assistant_reply_modes.rs`, schema 141), which still uses
//! the two columns this step added: `instruction` and `anchor`.
pub(crate) use migration::ThreadTakeOver;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct ThreadTakeOver;
    impl MigrationName for ThreadTakeOver {
        fn name(&self) -> &str {
            "m20261119_000136_assistant_thread_takeover"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for ThreadTakeOver {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("assistant_thread_takeover.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Thread take-over requires a forward migration".into(),
            ))
        }
    }
}
