//! Tool requests use the same authenticated exact assignment/session as output and renewals.
use super::*;
use crate::identity::{
    control::broker::RemoteRequest,
    postgres::worker_control::{ControlDenied, Work},
};
pub(super) async fn invoke(
    State(state): State<StateData>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<RemoteRequest>,
) -> Api<Value> {
    super::super::postgres::worker_control::require(id == input.assignment.scope.assignment)?;
    let hash = credential(&headers)?;
    let record = pg(&state)
        .controlled_assignment(&input.assignment.scope, &hash)
        .await?;
    super::super::postgres::worker_control::require(
        record.session.as_deref() == Some(&input.assignment.session),
    )?;
    // Executor tasks are tool-free; only chat replies carry a tool broker.
    let Work::Reply(assignment) = &record.work else {
        return Err(ControlDenied.into());
    };
    Ok(Json(
        crate::identity::control::broker::invoke(
            pg(&state),
            &assignment.job,
            &input.tool,
            input.arguments,
        )
        .await?,
    ))
}
