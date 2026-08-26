use libmir::{ProgressEvent, ProgressStage, ProgressUnit};
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
    let operation = service.activity.begin("load", &requested, None);
    operation.progress("resolving", "resolving model", Some(0), None);
    tracing::info!(model = %requested, "model load requested");
    send(
        sender,
        event(
            operation.id(),
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
            operation.finish("failed", error.message());
            drop(sender.blocking_send(Err(error)));
            return;
        },
    };
    operation.progress(
        "checking_memory",
        "checking weights, KV cache, workspace, and device budget",
        Some(0),
        None,
    );
    send(sender, checking_memory(operation.id(), &selector));
    let mut progress = |progress: ProgressEvent| {
        log_progress(&selector, &progress);
        let phase = match progress.stage {
            ProgressStage::LoadWeights
                if progress.total > 0 && progress.current >= progress.total =>
            {
                "initializing"
            },
            ProgressStage::LoadWeights => "loading",
            ProgressStage::PrefillTokens => "warming",
            ProgressStage::DecodeTokens => "decoding",
        };
        let unit = match progress.unit {
            ProgressUnit::Byte => "byte",
            ProgressUnit::Token => "token",
        };
        operation.progress(phase, &progress.detail, Some(progress.current), Some(progress.total));
        send(
            sender,
            event(
                operation.id(),
                &selector,
                phase,
                LifecycleState {
                    current: progress.current,
                    total: Some(progress.total),
                    unit,
                    detail: &progress.detail,
                    model: None,
                },
            ),
        );
    };
    match service.load_model(&selector, request.force, &mut progress) {
        Ok(entry) => {
            operation.finish("completed", "model is ready");
            tracing::info!(model = %entry.info.id, path = %entry.info.path, "model is ready");
            send(
                sender,
                event(
                    operation.id(),
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
            let status = super::load_error(&error);
            operation.finish("failed", status.message());
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
