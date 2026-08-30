use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use libmir::CancellationToken;
use tokio::sync::broadcast;

mod types;

pub use types::{ActivityKind, ActivityOutcome, ActivityProgress, ActivityStage, ActivityStatus};

const HISTORY_LIMIT: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityEvent {
    pub operation_id: String,
    pub kind: ActivityKind,
    pub target: String,
    pub status: ActivityStatus,
    pub detail: String,
    pub started_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub cancellable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelOutcome {
    NotFound,
    NotCancellable(ActivityStatus),
    Requested,
}

#[derive(Clone)]
pub struct Activity {
    inner: Arc<Mutex<State>>,
    events: broadcast::Sender<ActivityEvent>,
}

struct State {
    next: u64,
    history: VecDeque<ActivityEvent>,
    cancellations: HashMap<String, CancellationToken>,
}

#[derive(Clone)]
pub struct Operation {
    activity: Activity,
    id: String,
}

impl Activity {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(128);
        Self {
            inner: Arc::new(Mutex::new(State {
                next: 1,
                history: VecDeque::new(),
                cancellations: HashMap::new(),
            })),
            events,
        }
    }

    pub fn begin(
        &self,
        kind: ActivityKind,
        target: &str,
        cancellation: Option<CancellationToken>,
    ) -> Operation {
        self.start(
            kind,
            target,
            ActivityStatus::Running {
                stage: ActivityStage::Starting,
                progress: None,
            },
            cancellation,
        )
    }

    pub fn enqueue(
        &self,
        kind: ActivityKind,
        target: &str,
        cancellation: CancellationToken,
    ) -> Operation {
        self.start(kind, target, ActivityStatus::Queued, Some(cancellation))
    }

    fn start(
        &self,
        kind: ActivityKind,
        target: &str,
        status: ActivityStatus,
        cancellation: Option<CancellationToken>,
    ) -> Operation {
        let mut state = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let id = format!("op-{}", state.next);
        state.next = state.next.saturating_add(1);
        if let Some(token) = cancellation {
            state.cancellations.insert(id.clone(), token);
        }
        let now = unix_ms();
        let event = ActivityEvent {
            operation_id: id.clone(),
            kind,
            target: target.to_owned(),
            status,
            detail: format!("{} {}", kind.as_str(), status.state_str()),
            started_at_unix_ms: now,
            updated_at_unix_ms: now,
            cancellable: state.cancellations.contains_key(&id),
        };
        record(&mut state, event.clone());
        drop(state);
        drop(self.events.send(event));
        Operation { activity: self.clone(), id }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ActivityEvent> {
        self.events.subscribe()
    }

    pub fn history(&self) -> Vec<ActivityEvent> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .history
            .iter()
            .cloned()
            .collect()
    }

    pub fn cancel(&self, id: &str) -> CancelOutcome {
        let mut state = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(index) = state.history.iter().position(|event| event.operation_id == id) else {
            return CancelOutcome::NotFound;
        };
        let Some(token) = state.cancellations.get(id).cloned() else {
            return CancelOutcome::NotCancellable(state.history[index].status);
        };
        token.cancel();
        let event = &mut state.history[index];
        let (phase, progress) = match event.status {
            ActivityStatus::Queued => (ActivityStage::Queued, None),
            ActivityStatus::Running { stage, progress }
            | ActivityStatus::Cancelling { stage, progress } => (stage, progress),
            ActivityStatus::Finished(_) => return CancelOutcome::NotCancellable(event.status),
        };
        event.status = ActivityStatus::Cancelling { stage: phase, progress };
        "cancellation requested".clone_into(&mut event.detail);
        event.updated_at_unix_ms = unix_ms();
        let event = event.clone();
        drop(state);
        drop(self.events.send(event));
        CancelOutcome::Requested
    }

    fn update(&self, id: &str, update: impl FnOnce(&mut ActivityEvent) -> bool) {
        let mut state = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(event) = state.history.iter_mut().find(|event| event.operation_id == id) else {
            return;
        };
        if !update(event) {
            return;
        }
        event.updated_at_unix_ms = unix_ms();
        if event.status.is_terminal() {
            event.cancellable = false;
        }
        let event = event.clone();
        if event.status.is_terminal() {
            state.cancellations.remove(id);
        }
        trim_history(&mut state);
        drop(state);
        drop(self.events.send(event));
    }
}

impl Operation {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn progress(&self, stage: ActivityStage, detail: &str, progress: Option<ActivityProgress>) {
        self.activity.update(&self.id, |event| {
            if !matches!(event.status, ActivityStatus::Queued | ActivityStatus::Running { .. }) {
                return false;
            }
            event.status = ActivityStatus::Running { stage, progress };
            detail.clone_into(&mut event.detail);
            true
        });
    }

    pub fn finish(&self, outcome: ActivityOutcome, detail: &str) {
        self.activity.update(&self.id, |event| {
            if event.status.is_terminal() {
                return false;
            }
            event.status = ActivityStatus::Finished(outcome);
            detail.clone_into(&mut event.detail);
            true
        });
    }
}

fn record(state: &mut State, event: ActivityEvent) {
    state.history.push_back(event);
    trim_history(state);
}

fn trim_history(state: &mut State) {
    while state.history.len() > HISTORY_LIMIT {
        let Some(index) = state.history.iter().position(|event| event.status.is_terminal()) else {
            break;
        };
        if let Some(removed) = state.history.remove(index) {
            state.cancellations.remove(&removed.operation_id);
        }
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests;
