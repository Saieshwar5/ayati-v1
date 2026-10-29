//! Trusted AWS bridge input; the public IAM gateway never reaches this loopback route directly.
use super::*;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CloudRegistration {
    pub registration: Registration,
    pub task_arn: String,
    pub principal_arn: String,
    pub release_arn: String,
}

impl CloudRegistration {
    fn validate(&self, cloud: Option<&crate::config::CloudWorkers>) -> Result<()> {
        let cloud = cloud.context("Cloud bridge is disabled")?;
        let (cluster, release, role) = (&cloud.cluster_arn, &cloud.release_arn, &cloud.role_arn);
        let (base, name) = cluster
            .rsplit_once(":cluster/")
            .context("Invalid cloud cluster")?;
        ensure!(
            base.starts_with("arn:aws:ecs:ap-south-1:") && name.starts_with("ayati-"),
            "Invalid cloud cluster"
        );
        let prefix = format!("{base}:task/{name}/");
        let id = self
            .task_arn
            .strip_prefix(&prefix)
            .context("Wrong cloud task")?;
        ensure!(
            id.len() == 32
                && id
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "Invalid cloud task ID"
        );
        let account = base.rsplit(':').next().unwrap();
        let role_name = role
            .strip_prefix(&format!("arn:aws:iam::{account}:role/"))
            .context("Wrong cloud role")?;
        ensure!(
            !role_name.contains('/'),
            "Cloud executor role must have a fixed root path"
        );
        ensure!(
            self.principal_arn == format!("arn:aws:sts::{account}:assumed-role/{role_name}/{id}")
                && &self.release_arn == release,
            "Wrong cloud principal or release"
        );
        self.registration.validate()
    }
}

pub(super) async fn register(
    State(state): State<StateData>,
    headers: HeaderMap,
    Json(input): Json<CloudRegistration>,
) -> Api<Value> {
    super::super::postgres::worker_control::require(credential(&headers)? == state.bootstrap_hash)?;
    input.validate(state.app.config.cloud.as_ref())?;
    super::super::postgres::worker_control::require(
        input.registration.pooled && pg(&state).pooled,
    )?;
    pg(&state).register_cloud_executor(&input).await?;
    Ok(Json(json!({"registered":true})))
}
