use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use crate::{application, rpc::proto};

pub fn watch(
    application: &application::Application,
    include_history: bool,
) -> ReceiverStream<Result<proto::ActivityEvent, Status>> {
    let (sender, receiver) = mpsc::channel(128);
    let mut events = application.activity_updates();
    if include_history {
        for event in application.activity_history() {
            if sender.try_send(Ok(event.into())).is_err() {
                break;
            }
        }
    }
    drop(tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    if sender.send(Ok(event.into())).await.is_err() {
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

pub fn event(value: application::ActivityEvent) -> proto::ActivityEvent {
    proto::ActivityEvent {
        operation_id: value.operation_id,
        kind: value.kind.as_str().to_owned(),
        target: value.target,
        state: value.state.as_str().to_owned(),
        stage: value.stage.as_str().to_owned(),
        detail: value.detail,
        started_at_unix_ms: value.started_at_unix_ms,
        updated_at_unix_ms: value.updated_at_unix_ms,
        cancellable: value.cancellable,
        current: value.current,
        total: value.total,
    }
}

impl From<application::ActivityEvent> for proto::ActivityEvent {
    fn from(value: application::ActivityEvent) -> Self {
        event(value)
    }
}

pub fn cancellation(value: application::CancelOutcome) -> proto::CancelOperationResponse {
    proto::CancelOperationResponse {
        found: value.found,
        accepted: value.accepted,
        state: value.state.as_str().to_owned(),
    }
}
