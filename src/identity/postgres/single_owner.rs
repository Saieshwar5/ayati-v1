//! One owner per agent (schema 122). The manager role is gone: an agent's
//! owner changes it, decides its "Ask first" actions, sees its work and pauses
//! it; admins have the same authority over every agent. When an owner leaves
//! the workspace and no new owner is chosen, the agent has no owner
//! (`owner_id` NULL) and is paused until an admin gives it one.
use super::*;

/// Whether schema 122 is applied. Upgrade tests run this code on older schemas.
pub(super) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(db, "SELECT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('relay_shared_agents') AND attname='resume_with_owner' AND NOT attisdropped) AS ready", vec![])
        .await?
        .context("Missing schema check")?
        .try_get("", "ready")?)
}

/// `person` is leaving: their agents go to `new_owner`, or lose their owner and
/// pause. A moved only-me agent pauses so its new owner looks at it first; an
/// agent left without an owner remembers whether it was running, to resume
/// when it gets one (an only-me agent stays paused for that same look).
pub(super) async fn owner_leaves(
    tx: &impl ConnectionTrait,
    w: &str,
    person: &str,
    new_owner: Option<&str>,
    heir: &str,
) -> Result<()> {
    // Before schema 122 an agent always had an owner: the person taking over.
    let new_owner = match new_owner {
        None if !ready(tx).await? => Some(heir),
        chosen => chosen,
    };
    match new_owner {
        Some(owner) => {
            execute(tx, "UPDATE relay_shared_agents SET owner_id=$3,revision=revision+1,settings=CASE WHEN audience='only_me' THEN jsonb_set(settings,'{paused}','true') ELSE settings END WHERE workspace=$1 AND owner_id=$2", vec![w.into(), person.into(), owner.into()]).await?;
        }
        None => {
            execute(tx, "UPDATE relay_shared_agents SET owner_id=NULL,revision=revision+1,
                resume_with_owner=(audience<>'only_me' AND COALESCE(settings->>'paused','false')<>'true'),
                settings=jsonb_set(settings,'{paused}','true') WHERE workspace=$1 AND owner_id=$2", vec![w.into(), person.into()]).await?;
        }
    }
    Ok(())
}

pub(crate) use migration::SingleOwner;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct SingleOwner;
    impl MigrationName for SingleOwner {
        fn name(&self) -> &str {
            "m20261115_000122_single_owner"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for SingleOwner {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("single_owner.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "One owner per agent requires a forward migration".into(),
            ))
        }
    }
}
