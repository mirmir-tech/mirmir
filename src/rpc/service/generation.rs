use std::time::Instant;

use libmir::{
    CancellationToken, ChatCompletionRequest, ChatMessage, GenerationChannel, ProgressStage,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use super::{
    RuntimeService, activity::Operation, models::log_progress, telemetry::GenerationTelemetry,
};
use crate::rpc::proto;

pub fn stream(
    service: RuntimeService,
    request: proto::GenerateRequest,
) -> ReceiverStream<Result<proto::GenerateEvent, Status>> {
    let (sender, receiver) = mpsc::channel(64);
    let cancellation = CancellationToken::new();
    let operation = service.activity.begin("generate", &request.model, Some(cancellation.clone()));
    drop(sender.try_send(Ok(proto::GenerateEvent {
        event: Some(proto::generate_event::Event::Started(proto::OperationStarted {
            operation_id: operation.id().to_owned(),
        })),
    })));
    drop(tokio::task::spawn_blocking(move || {
        run(&service, &request, &sender, &operation, &cancellation);
    }));
    ReceiverStream::new(receiver)
}

fn run(
    service: &RuntimeService,
    request: &proto::GenerateRequest,
    sender: &mpsc::Sender<Result<proto::GenerateEvent, Status>>,
    operation: &Operation,
    cancellation: &CancellationToken,
) {
    let mut telemetry = service.telemetry.begin();
    let started = Instant::now();
    tracing::info!(
        operation = operation.id(),
        model = %request.model,
        messages = request.messages.len(),
        max_tokens = request.max_tokens,
        "generation started"
    );
    let model = match load_for_generation(service, &request.model, operation, &telemetry) {
        Ok(model) => model,
        Err(status) => {
            telemetry.fail();
            operation.finish("failed", status.message());
            tracing::warn!(operation = operation.id(), model = %request.model, error = %status, "generation could not start");
            drop(sender.blocking_send(Err(status)));
            return;
        },
    };
    let chat = match chat_request(request) {
        Ok(chat) => chat,
        Err(status) => {
            telemetry.fail();
            operation.finish("failed", status.message());
            tracing::warn!(operation = operation.id(), model = %request.model, error = %status, "generation rejected");
            drop(sender.blocking_send(Err(status)));
            return;
        },
    };
    let mut ttft_ms = None;
    telemetry.stage("prefill");
    operation.progress("prefill", "prefilling prompt", None, None);
    let progress_operation = operation.clone();
    let output = model.generate_cancellable(
        &chat,
        &mut |event| {
            telemetry.progress(&event);
            let stage = match event.stage {
                ProgressStage::LoadWeights => "loading",
                ProgressStage::PrefillTokens => "prefill",
                ProgressStage::DecodeTokens => "decode",
            };
            if event.stage != ProgressStage::DecodeTokens
                || event.current == 0
                || event.current == event.total
                || event.current % 16 == 0
            {
                progress_operation.progress(
                    stage,
                    &event.detail,
                    Some(event.current),
                    Some(event.total),
                );
            }
        },
        &mut |token| {
            telemetry.token_emitted();
            ttft_ms.get_or_insert_with(|| started.elapsed().as_secs_f64() * 1_000.0);
            let event = proto::GenerateEvent {
                event: Some(proto::generate_event::Event::Token(proto::Token {
                    id: token.id,
                    text: token.text,
                    reasoning: token.channel == GenerationChannel::Reasoning,
                })),
            };
            if sender.blocking_send(Ok(event)).is_err() {
                cancellation.cancel();
            }
        },
        cancellation,
    );
    match output {
        Ok(output) => {
            send_completion(
                sender, output, started, ttft_ms, &mut telemetry, operation, &request.model,
            );
        },
        Err(libmir::Error::Cancelled) => {
            telemetry.fail();
            operation.finish("cancelled", "generation cancelled");
            tracing::info!(operation = operation.id(), model = %request.model, "generation cancelled");
            drop(sender.blocking_send(Err(Status::cancelled("generation cancelled"))));
        },
        Err(error) => {
            telemetry.fail();
            operation.finish("failed", &error.to_string());
            tracing::error!(operation = operation.id(), model = %request.model, %error, "generation failed");
            drop(sender.blocking_send(Err(Status::internal(error.to_string()))));
        },
    }
}

fn load_for_generation(
    service: &RuntimeService,
    selector: &str,
    operation: &Operation,
    telemetry: &GenerationTelemetry,
) -> Result<libmir::Model, Status> {
    telemetry.stage("resolving");
    operation.progress("resolving", "resolving model", None, None);
    let load_operation = operation.clone();
    let mut progress = |event: libmir::ProgressEvent| {
        log_progress(selector, &event);
        telemetry.progress(&event);
        let stage = match event.stage {
            ProgressStage::LoadWeights => "loading",
            ProgressStage::PrefillTokens => "warming",
            ProgressStage::DecodeTokens => "decoding",
        };
        load_operation.progress(stage, &event.detail, Some(event.current), Some(event.total));
    };
    service.load(selector, false, &mut progress).map(|entry| entry.model)
}

fn send_completion(
    sender: &mpsc::Sender<Result<proto::GenerateEvent, Status>>,
    output: libmir::GenerationOutput,
    started: Instant,
    ttft_ms: Option<f64>,
    telemetry: &mut GenerationTelemetry,
    operation: &Operation,
    model: &str,
) {
    let elapsed = started.elapsed().as_secs_f64();
    let tokens = output.token_ids.len();
    let tokens_per_second = tokens
        .to_string()
        .parse::<f64>()
        .ok()
        .filter(|_| elapsed > 0.0)
        .map(|tokens| tokens / elapsed);
    let completion = proto::Completion {
        reasoning: output.reasoning,
        prefill_tokens_per_second: output.metrics.throughput.prefill.per_second,
        decode_tokens_per_second: output.metrics.throughput.decode.per_second,
        prefill_ms: Some(output.metrics.durations_ms.prefill),
        decode_ms: Some(output.metrics.durations_ms.decode),
        text: output.text,
        prompt_tokens: u64::try_from(output.prompt_tokens).unwrap_or(u64::MAX),
        completion_tokens: u64::try_from(tokens).unwrap_or(u64::MAX),
        finish_reason: output.finish_reason.to_owned(),
        elapsed_ms: elapsed * 1_000.0,
        tokens_per_second,
        ttft_ms,
    };
    telemetry.complete(&completion);
    operation.finish("completed", "generation completed");
    tracing::info!(
        operation = operation.id(),
        model = %model,
        prompt_tokens = completion.prompt_tokens,
        completion_tokens = completion.completion_tokens,
        finish_reason = %completion.finish_reason,
        elapsed_ms = completion.elapsed_ms,
        prefill_tokens_per_second = completion.prefill_tokens_per_second,
        decode_tokens_per_second = completion.decode_tokens_per_second,
        "generation completed"
    );
    drop(sender.blocking_send(Ok(proto::GenerateEvent {
        event: Some(proto::generate_event::Event::Completion(completion)),
    })));
}

fn chat_request(request: &proto::GenerateRequest) -> Result<ChatCompletionRequest, Status> {
    Ok(ChatCompletionRequest {
        model: request.model.clone(),
        messages: messages(request),
        stream: true,
        max_tokens: request.max_tokens.map(usize::try_from).transpose().map_err(integer_status)?,
        temperature: request.temperature,
        top_p: request.top_p,
        top_k: request.top_k.map(usize::try_from).transpose().map_err(integer_status)?,
        repetition_penalty: request.repetition_penalty,
        seed: request.seed,
    })
}

fn messages(request: &proto::GenerateRequest) -> Vec<ChatMessage> {
    if request.messages.is_empty() {
        vec![ChatMessage {
            role: "user".to_owned(),
            content: request.prompt.clone(),
            reasoning_content: None,
        }]
    } else {
        request
            .messages
            .iter()
            .map(|message| ChatMessage {
                role: message.role.clone(),
                content: message.content.clone(),
                reasoning_content: message.reasoning_content.clone(),
            })
            .collect()
    }
}

fn integer_status(error: std::num::TryFromIntError) -> Status {
    Status::invalid_argument(error.to_string())
}
