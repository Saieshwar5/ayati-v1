//! How an invitation reaches the person (schema 93).
//!
//! `email`: WorkOS emailed it. The invitation becomes usable once WorkOS confirms
//! with its own invitation id (`provider`).
//! `link`: WorkOS definitely refused to email it (a 4xx answer, so nothing exists at
//! WorkOS; for example the address already has an account). The admin sends the
//! Cuckoo link instead, and the invited person also finds it on their own page.
//! An uncertain outcome (timeout, 5xx, an odd answer) stays `delivery_unknown`, as
//! before: that invitation is not usable until an admin cancels and resends it.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

pub(crate) struct InvitationDelivery;
impl MigrationName for InvitationDelivery {
    fn name(&self) -> &str {
        "m20261107_000093_invitation_delivery"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for InvitationDelivery {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        // A constant default is a metadata-only change; existing rows read 'email'.
        m.get_connection()
            .execute_unprepared(
                "ALTER TABLE relay_invitations ADD COLUMN delivery TEXT NOT NULL DEFAULT 'email'
                  CHECK (delivery IN ('email','link'));",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Invitation delivery requires a forward migration".into(),
        ))
    }
}

// An invitation is usable once its delivery is settled:
// `provider IS NOT NULL OR delivery='link'` (accept in people.rs, the invited
// person's list in accounts.rs).
impl PgStore {
    /// WorkOS refused the email for certain: the invitation stays pending, link only.
    pub async fn link_only(&self, id: &str) -> Result<()> {
        let changed = execute(
            &self.db,
            "UPDATE relay_invitations SET delivery='link' WHERE id=$1 AND status='pending' AND provider IS NULL",
            vec![id.into()],
        )
        .await?;
        ensure!(changed == 1, "Invitation unavailable");
        Ok(())
    }
}
