use libmir::ProgressEvent;
use tokio::sync::mpsc;
use tonic::Status;

use super::{
    events::{LifecycleState, checking_memory, event},
    log_progress,
};
use crate::rpc::{proto, service::RuntimeService};

pub fn stream_load(
    service: &RuntimeService,
    request: &proto::LoadModelRequest,
    sender: &mpsc::Sender<Result<proto::ModelLifecycleEvent, Status>>,
) {
    let requested = request.selector.clone();
    let session = service.application.start_model_load(&requested);
    tracing::info!(model = %requested, "model load requested");
    send(
        sender,
        event(
            session.operation_id(),
            &requested,
            "resolving",
            LifecycleState {
                current: 0,
                total: None,
                unit: "item",
                detail: "resolving model",
                model: None,
            },
        ),
    );
    let selector = match service.prepare_load(request) {
        Ok(selector) => selector,
        Err(error) => {
            session.reject(error.message());
            drop(sender.blocking_send(Err(error)));
            return;
        },
    };
    session.checking_memory();
    send(sender, checking_memory(session.operation_id(), &selector));
    let mut progress = |progress: ProgressEvent| {
        log_progress(&selector, &progress);
        send(
            sender,
            event(
                session.operation_id(),
                &selector,
                progress.stage.as_str(),
                LifecycleState {
                    current: progress.current,
                    total: Some(progress.total),
                    unit: progress.unit.as_str(),
                    detail: &progress.detail,
                    model: None,
                },
            ),
        );
    };
    match service
        .application
        .load_model_session(&session, &selector, request.force, &mut progress)
    {
        Ok(entry) => {
            tracing::info!(model = %entry.info.id, path = %entry.info.path, "model is ready");
            send(
                sender,
                event(
                    session.operation_id(),
                    &requested,
                    "ready",
                    LifecycleState {
                        current: 1,
                        total: Some(1),
                        unit: "item",
                        detail: "model is ready",
                        model: Some(entry.info.into()),
                    },
                ),
            );
        },
        Err(error) => {
            let status = super::super::status::application_ref(&error);
            tracing::error!(model = %selector, %error, "model load failed");
            drop(sender.blocking_send(Err(status)));
        },
    }
}

fn send(
    sender: &mpsc::Sender<Result<proto::ModelLifecycleEvent, Status>>,
    event: proto::ModelLifecycleEvent,
) {
    drop(sender.blocking_send(Ok(event)));
}
