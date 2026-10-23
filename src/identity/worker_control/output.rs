//! Ordered, bounded visible text only. Never a tool, reasoning or authority channel.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Update {
    pub assignment: AssignedRequest,
    pub sequence: i64,
    pub phase: String,
    pub text: String,
}
impl Update {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=4096).contains(&self.sequence),
            "Output sequence exceeds limit"
        );
        ensure!(
            ["waiting", "responding"].contains(&self.phase.as_str()),
            "Invalid output phase"
        );
        ensure!(
            self.text.len() <= 4096 && !self.text.contains('\0'),
            "Invalid output batch"
        );
        ensure!(
            if self.phase == "waiting" {
                self.text.is_empty() && self.sequence == 1
            } else {
                !self.text.is_empty()
            },
            "Invalid phase content"
        );
        Ok(())
    }
}
pub(super) async fn append(
    State(state): State<StateData>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<Update>,
) -> Api<Value> {
    super::super::postgres::worker_control::require(id == input.assignment.scope.assignment)?;
    let sequence = pg(&state)
        .append_output(&input, &credential(&headers)?)
        .await?;
    Ok(Json(json!({"sequence":sequence})))
}
