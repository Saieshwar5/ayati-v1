//! Persistent dispatch stream and capacity API for the opt-in local executor pool.
use super::*;
use crate::identity::{executor::model::Capacity, postgres::PoolLimits};
use axum::{
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use std::{
    convert::Infallible,
    pin::Pin,
    task::{Context as TaskContext, Poll},
    time::Duration,
};
use tokio_stream::{Stream, wrappers::ReceiverStream};

struct Offers {
    receiver: ReceiverStream<Result<Event, Infallible>>,
    state: StateData,
    key: (String, String),
    connection: String,
}
impl Stream for Offers {
    type Item = Result<Event, Infallible>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_next(cx)
    }
}
impl Drop for Offers {
    fn drop(&mut self) {
        self.state.hub.detach(&self.key, &self.connection);
    }
}
pub(super) fn routes() -> Router<StateData> {
    Router::new()
        .route(
            "/v1/executors/{executor}/{incarnation}/pool/events",
            get(events),
        )
        .route(
            "/v1/executors/{executor}/{incarnation}/pool/capacity",
            post(capacity),
        )
        .route(
            "/v1/executors/{executor}/{incarnation}/pool/drain",
            post(drain),
        )
        .route(
            "/v1/executors/{executor}/{incarnation}/pool/stop",
            post(stop),
        )
}
async fn authorize(
    state: &Control,
    executor: &str,
    incarnation: &str,
    headers: &HeaderMap,
) -> Result<()> {
    super::super::postgres::worker_control::require(pg(state).pooled)?;
    pg(state)
        .executor_allowed(executor, incarnation, &credential(headers)?)
        .await?;
    pg(state).pooled_executor(executor, incarnation).await
}
async fn events(
    State(state): State<StateData>,
    Path(key): Path<(String, String)>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, Error> {
    authorize(&state, &key.0, &key.1, &headers).await?;
    let (connection, receiver) = state.hub.attach(key.clone())?;
    Ok(Sse::new(Offers {
        receiver: ReceiverStream::new(receiver),
        state,
        key,
        connection,
    })
    .keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(1))
            .text("executor"),
    ))
}
async fn capacity(
    State(state): State<StateData>,
    Path((executor, incarnation)): Path<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Capacity>,
) -> Api<Value> {
    authorize(&state, &executor, &incarnation, &headers).await?;
    pg(&state)
        .report_executor(&executor, &incarnation, &input)
        .await?;
    state.hub.wake.notify_one();
    Ok(Json(json!({"reported":true})))
}
async fn drain(
    State(state): State<StateData>,
    Path((executor, incarnation)): Path<(String, String)>,
    headers: HeaderMap,
) -> Api<Vec<Scope>> {
    authorize(&state, &executor, &incarnation, &headers).await?;
    pg(&state).drain_executor(&executor, &incarnation).await?;
    state.hub.wake.notify_one();
    Ok(Json(
        pg(&state).pending_offers(&executor, &incarnation).await?,
    ))
}
async fn stop(
    State(state): State<StateData>,
    Path((executor, incarnation)): Path<(String, String)>,
    headers: HeaderMap,
) -> Api<Value> {
    authorize(&state, &executor, &incarnation, &headers).await?;
    pg(&state).stop_executor(&executor, &incarnation).await?;
    Ok(Json(json!({"stopped":true})))
}
pub(super) async fn dispatch(state: StateData, limits: PoolLimits, expiry: u64) {
    let stopping = state.app.runtimes.stopping.clone();
    loop {
        if pg(&state).expire_executors(expiry).await.is_err() {
            tracing::warn!("Executor registry unavailable; admission paused");
        }
        for key in state.hub.connected() {
            if stopping.is_cancelled() {
                return;
            }
            if let Ok(offers) = pg(&state).pending_offers(&key.0, &key.1).await {
                for offer in offers {
                    super::super::start_trace::point(
                        &offer.assignment,
                        "controller",
                        "offer_available",
                    );
                    state.hub.offer(&key, &offer);
                }
            }
            for _ in 0..16 {
                match pg(&state).reserve_ready_slot(&key.0, &key.1, limits).await {
                    Ok(Some(scope)) => {
                        super::super::start_trace::point(
                            &scope.assignment,
                            "controller",
                            "offer_available",
                        );
                        state.hub.offer(&key, &scope);
                    }
                    Ok(None) => break,
                    Err(_) => {
                        tracing::warn!("Executor dispatch deferred; ownership remains durable");
                        break;
                    }
                }
            }
        }
        tokio::select! {
            _=stopping.cancelled()=>break,
            _=state.hub.wake.notified()=>{},
            _=state.app.runtimes.chat_wake.notified()=>{},
            _=tokio::time::sleep(Duration::from_millis(200))=>{},
        }
    }
}
