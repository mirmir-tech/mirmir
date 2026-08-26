use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use libmir::CancellationToken;
use tokio::sync::broadcast;

const HISTORY_LIMIT: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityEvent {
    pub operation_id: String,
    pub kind: String,
    pub target: String,
    pub state: String,
    pub stage: String,
    pub detail: String,
    pub started_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub cancellable: bool,
    pub current: Option<u64>,
    pub total: Option<u64>,
}

pub struct CancelOutcome {
    pub found: bool,
    pub accepted: bool,
    pub state: String,
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
        kind: &str,
        target: &str,
        cancellation: Option<CancellationToken>,
    ) -> Operation {
        self.begin_with_state(kind, target, "running", cancellation)
    }

    pub fn enqueue(&self, kind: &str, target: &str, cancellation: CancellationToken) -> Operation {
        self.begin_with_state(kind, target, "queued", Some(cancellation))
    }

    fn begin_with_state(
        &self,
        kind: &str,
        target: &str,
        state_name: &str,
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
            kind: kind.to_owned(),
            target: target.to_owned(),
            state: state_name.to_owned(),
            stage: state_name.to_owned(),
            detail: format!("{kind} {state_name}"),
            started_at_unix_ms: now,
            updated_at_unix_ms: now,
            cancellable: state.cancellations.contains_key(&id),
            current: None,
            total: None,
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
            return CancelOutcome {
                found: false,
                accepted: false,
                state: "not_found".to_owned(),
            };
        };
        let Some(token) = state.cancellations.get(id).cloned() else {
            return CancelOutcome {
                found: true,
                accepted: false,
                state: state.history[index].state.clone(),
            };
        };
        token.cancel();
        let event = &mut state.history[index];
        "cancelling".clone_into(&mut event.state);
        "cancellation requested".clone_into(&mut event.detail);
        event.updated_at_unix_ms = unix_ms();
        let event = event.clone();
        drop(state);
        drop(self.events.send(event));
        CancelOutcome {
            found: true,
            accepted: true,
            state: "cancelling".to_owned(),
        }
    }

    fn update(&self, id: &str, update: impl FnOnce(&mut ActivityEvent)) {
        let mut state = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(event) = state.history.iter_mut().find(|event| event.operation_id == id) else {
            return;
        };
        update(event);
        event.updated_at_unix_ms = unix_ms();
        let event = event.clone();
        if terminal(&event.state) {
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

    pub fn progress(&self, stage: &str, detail: &str, current: Option<u64>, total: Option<u64>) {
        self.activity.update(&self.id, |event| {
            if event.state == "queued" && stage != "queued" {
                "running".clone_into(&mut event.state);
            }
            stage.clone_into(&mut event.stage);
            detail.clone_into(&mut event.detail);
            event.current = current;
            event.total = total;
        });
    }

    pub fn finish(&self, state: &str, detail: &str) {
        self.activity.update(&self.id, |event| {
            state.clone_into(&mut event.state);
            state.clone_into(&mut event.stage);
            detail.clone_into(&mut event.detail);
        });
    }
}

fn record(state: &mut State, event: ActivityEvent) {
    state.history.push_back(event);
    trim_history(state);
}

fn trim_history(state: &mut State) {
    while state.history.len() > HISTORY_LIMIT {
        let Some(index) = state.history.iter().position(|event| terminal(&event.state)) else {
            break;
        };
        if let Some(removed) = state.history.remove(index) {
            state.cancellations.remove(&removed.operation_id);
        }
    }
}

fn terminal(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "cancelled")
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests;
