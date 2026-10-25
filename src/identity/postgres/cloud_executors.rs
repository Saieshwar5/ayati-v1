//! Durable ECS identity binding. Only the trusted loopback bootstrap may establish it.
use super::worker_control::require;
use super::*;
use crate::identity::worker_control::{cloud::CloudRegistration, model::Registration};

impl PgStore {
    pub(crate) async fn register_cloud_executor(&self, input: &CloudRegistration) -> Result<()> {
        input.registration.validate()?;
        let tx = self.db.begin().await?;
        let registration = &input.registration;
        // The conflict check locks the executor row and keeps its registration
        // mode immutable, including when local/cloud registrations race.
        register_in(&tx, registration, true).await?;
        let identity = vec![
            registration.executor.clone().into(),
            registration.incarnation.clone().into(),
            input.task_arn.clone().into(),
            input.principal_arn.clone().into(),
            input.release_arn.clone().into(),
        ];
        execute(&tx, "INSERT INTO relay_cloud_executors(executor,incarnation,task_arn,principal_arn,release_arn)
            VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING", identity.clone()).await?;
        require(
            query(
                &tx,
                "SELECT executor FROM relay_cloud_executors WHERE executor=$1 AND incarnation=$2
            AND task_arn=$3 AND principal_arn=$4 AND release_arn=$5",
                identity,
            )
            .await?
            .is_some(),
        )?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn cloud_credential_allowed(
        &self,
        hash: &str,
        task: Option<&str>,
    ) -> Result<()> {
        let owners = rows(
            &self.db,
            "SELECT c.task_arn FROM (
            SELECT id AS executor FROM relay_executors WHERE token_hash=$1
            UNION SELECT executor FROM relay_assignment_credentials WHERE token_hash=$1
            ) owner LEFT JOIN relay_cloud_executors c ON c.executor=owner.executor",
            vec![hash.into()],
        )
        .await?;
        if owners.is_empty() {
            // Preserve legacy route-specific credential denial. Cloud calls always need a binding.
            return require(task.is_none());
        }
        for owner in owners {
            require(owner.try_get::<Option<String>>("", "task_arn")?.as_deref() == task)?;
        }
        Ok(())
    }
}

pub(super) async fn register_in(
    db: &impl ConnectionTrait,
    registration: &Registration,
    cloud_bound: bool,
) -> Result<()> {
    registration.validate()?;
    let clock = super::worker_control::CLOCK;
    let identity = vec![
        registration.executor.clone().into(),
        registration.incarnation.clone().into(),
        super::super::provider::digest(&registration.token).into(),
        registration.pooled.into(),
        cloud_bound.into(),
    ];
    // Both unique identities (executor ID and credential hash) are conflicts,
    // rather than database outages. The guarded update also locks exact replays.
    execute(
        db,
        &format!(
            "INSERT INTO relay_executors(id,incarnation,token_hash,expires,pooled,cloud_bound)
        VALUES($1,$2,$3,{clock}+86400,$4,$5) ON CONFLICT DO NOTHING"
        ),
        identity.clone(),
    )
    .await?;
    let changed = execute(
        db,
        &format!(
            "UPDATE relay_executors SET expires={clock}+86400
        WHERE id=$1 AND incarnation=$2 AND token_hash=$3 AND pooled=$4 AND cloud_bound=$5"
        ),
        identity,
    )
    .await?;
    require(changed == 1)
}
