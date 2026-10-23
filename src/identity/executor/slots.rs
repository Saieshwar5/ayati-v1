//! Local slot state owns its one-use harness; credentials stay outside Goose.
use super::{
    harness::Harness,
    model::{Profile, SlotReport},
};
use crate::identity::{
    chat::execution,
    provider::random_token,
    worker_control::{
        attempt,
        client::Client,
        model::Registration,
        model::{AssignedRequest, Scope},
    },
};
use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
pub(super) enum State {
    Preparing,
    Ready(Box<Harness>),
    Running(Box<Scope>),
    Closed,
}
pub(super) struct Slot {
    pub id: String,
    pub generation: i64,
    pub token: String,
    pub profile: Profile,
    pub state: State,
    pub pid: Option<u32>,
}
pub(super) enum Done {
    Prepared(usize, i64, Result<Harness>),
    Finished(usize, i64),
}
pub(super) struct Control {
    pub client: Client,
    pub registration: Registration,
    pub url: String,
    pub stop: CancellationToken,
}
impl Slot {
    pub fn new(profile: Profile) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            generation: 1,
            token: random_token(),
            profile,
            state: State::Preparing,
            pid: None,
        }
    }
    pub fn report(&self) -> Option<SlotReport> {
        let state = match &self.state {
            State::Preparing => "preparing",
            State::Ready(_) => "ready",
            State::Running(_) => "running",
            State::Closed => return None,
        };
        Some(SlotReport {
            slot: self.id.clone(),
            generation: self.generation,
            profile: self.profile.clone(),
            state: state.into(),
            token: self.token.clone(),
            pid: self.pid,
        })
    }
    pub fn prepare(
        &self,
        index: usize,
        root: &Path,
        stop: &CancellationToken,
        jobs: &mut JoinSet<Done>,
    ) {
        let generation = self.generation;
        let profile = self.profile.clone();
        let stop = stop.clone();
        let root = root.join(&self.id).join(generation.to_string());
        jobs.spawn(async move {
            let result=tokio::select! {_=stop.cancelled()=>Err(anyhow::anyhow!("Executor draining")),result=Harness::prepare(&root,&profile)=>result};
            if result.is_err() {execution::cleanup(&root).await;tokio::time::sleep(std::time::Duration::from_millis(300)).await;}
            Done::Prepared(index,generation,result)
        });
    }
    pub fn next(&mut self) {
        self.generation += 1;
        self.token = random_token();
        self.pid = None;
        self.state = State::Preparing;
    }
    pub fn start(
        &mut self,
        index: usize,
        scope: Scope,
        control: &Control,
        jobs: &mut JoinSet<Done>,
    ) {
        let State::Ready(harness) = std::mem::replace(&mut self.state, State::Closed) else {
            return;
        };
        let root = harness.root();
        let shutdown = harness.shutdown_handle();
        let generation = self.generation;
        let run_client =
            Client::new(&control.url, &self.token).expect("Validated local capability");
        let client = control.client.clone();
        let registration = control.registration.clone();
        let stop = control.stop.clone();
        let request = AssignedRequest {
            scope: scope.clone(),
            session: uuid::Uuid::new_v4().to_string(),
        };
        self.state = State::Running(Box::new(scope.clone()));
        jobs.spawn(async move {
            let result=attempt::run(run_client,request,stop,|context,cancel|async move {harness.run(context,cancel).await}).await;
            let _=shutdown.shutdown().await;
            execution::cleanup(&root).await;
            if client.release(&registration,&scope,"lost").await.is_err() {tracing::warn!(assignment=%scope.assignment,"Pool release deferred to durable reconciliation");}
            tracing::info!(assignment=%scope.assignment,success=result.is_ok(),"Executor session stopped and cleared");
            Done::Finished(index,generation)
        });
    }
}
pub(super) async fn close(slots: &mut [Slot]) {
    for slot in slots {
        if let State::Ready(harness) = std::mem::replace(&mut slot.state, State::Closed) {
            harness.close().await;
        }
    }
}
pub(super) fn root(data: &Path) -> PathBuf {
    data.join("executor-slots")
}
