use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use libmir::CancellationToken;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use crate::rpc::proto;

const HISTORY_LIMIT: usize = 100;

#[derive(Clone)]
pub struct Activity {
    inner: Arc<Mutex<State>>,
    events: broadcast::Sender<proto::ActivityEvent>,
}

struct State {
    next: u64,
    history: VecDeque<proto::ActivityEvent>,
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
        let mut state = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let id = format!("op-{}", state.next);
        state.next = state.next.saturating_add(1);
        if let Some(token) = cancellation {
            state.cancellations.insert(id.clone(), token);
        }
        let now = unix_ms();
        let event = proto::ActivityEvent {
            operation_id: id.clone(),
            kind: kind.to_owned(),
            target: target.to_owned(),
            state: "running".to_owned(),
            stage: "starting".to_owned(),
            detail: format!("{kind} started"),
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

    pub fn watch(
        &self,
        include_history: bool,
    ) -> ReceiverStream<Result<proto::ActivityEvent, Status>> {
        let (sender, receiver) = mpsc::channel(128);
        let mut events = self.events.subscribe();
        if include_history {
            let state = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            for event in &state.history {
                if sender.try_send(Ok(event.clone())).is_err() {
                    break;
                }
            }
        }
        drop(tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if sender.send(Ok(event)).await.is_err() {
                            break;
                        }
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => {},
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }));
        ReceiverStream::new(receiver)
    }

    pub fn history(&self) -> Vec<proto::ActivityEvent> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .history
            .iter()
            .cloned()
            .collect()
    }

    pub fn cancel(&self, id: &str) -> proto::CancelOperationResponse {
        let mut state = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(index) = state.history.iter().position(|event| event.operation_id == id) else {
            return proto::CancelOperationResponse {
                found: false,
                accepted: false,
                state: "not_found".to_owned(),
            };
        };
        let Some(token) = state.cancellations.get(id).cloned() else {
            return proto::CancelOperationResponse {
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
        proto::CancelOperationResponse {
            found: true,
            accepted: true,
            state: "cancelling".to_owned(),
        }
    }

    fn update(&self, id: &str, update: impl FnOnce(&mut proto::ActivityEvent)) {
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

fn record(state: &mut State, event: proto::ActivityEvent) {
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
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_idempotent_and_updates_activity() {
        let activity = Activity::new();
        let token = CancellationToken::new();
        let operation = activity.begin("generate", "model", Some(token.clone()));
        assert!(activity.cancel(operation.id()).accepted);
        assert!(activity.cancel(operation.id()).accepted);
        assert!(token.is_cancelled());
        operation.finish("cancelled", "cancelled by test");
        assert!(!activity.cancel(operation.id()).accepted);
    }

    #[test]
    fn history_limit_never_evicts_a_running_operation() {
        let activity = Activity::new();
        let oldest = activity.begin("generate", "oldest", Some(CancellationToken::new()));
        for index in 0..HISTORY_LIMIT {
            let operation = activity.begin("load", &index.to_string(), None);
            operation.finish("completed", "done");
        }
        assert!(activity.cancel(oldest.id()).accepted);
    }
}
