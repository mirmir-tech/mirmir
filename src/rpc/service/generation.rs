use libmir::GenerationChannel;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use super::RuntimeService;
use crate::{application::GenerationResult, rpc::proto};

#[path = "generation/request.rs"]
mod request_conversion;

impl RuntimeService {
    pub(crate) fn generate_stream(
        &self,
        request: proto::GenerateRequest,
    ) -> ReceiverStream<Result<proto::GenerateEvent, Status>> {
        stream(self.clone(), request)
    }
}

pub fn stream(
    service: RuntimeService,
    request: proto::GenerateRequest,
) -> ReceiverStream<Result<proto::GenerateEvent, Status>> {
    let (sender, receiver) = mpsc::channel(64);
    let session = service.application.start_generation(&request.model);
    drop(sender.try_send(Ok(proto::GenerateEvent {
        event: Some(proto::generate_event::Event::Started(proto::OperationStarted {
            operation_id: session.operation_id().to_owned(),
        })),
    })));
    drop(tokio::task::spawn_blocking(move || run(&service, &request, &sender, session)));
    ReceiverStream::new(receiver)
}

fn run(
    service: &RuntimeService,
    request: &proto::GenerateRequest,
    sender: &mpsc::Sender<Result<proto::GenerateEvent, Status>>,
    mut session: crate::application::GenerationSession,
) {
    let chat = match request_conversion::chat_request(request) {
        Ok(chat) => chat,
        Err(status) => {
            session.reject(status.message());
            drop(sender.blocking_send(Err(status)));
            return;
        },
    };
    let mut progress = |_event| {};
    let cancellation = session.cancellation();
    let mut emit_token = |token: libmir::GenerationToken| {
        let event = proto::GenerateEvent {
            event: Some(proto::generate_event::Event::Token(proto::Token {
                id: token.id,
                text: token.text,
                reasoning: token.channel == GenerationChannel::Reasoning,
                channel: token_channel(token.channel).into(),
            })),
        };
        if sender.blocking_send(Ok(event)).is_err() {
            cancellation.cancel();
        }
    };
    let result = service.application.generate(
        &mut session,
        &chat,
        request.image.as_deref(),
        &mut progress,
        &mut emit_token,
    );
    match result {
        Ok(result) => send_completion(sender, result, &request.model, session.operation_id()),
        Err(error) => {
            let status = generation_error(error);
            tracing::warn!(operation = session.operation_id(), model = %request.model, %status, "generation failed");
            drop(sender.blocking_send(Err(status)));
        },
    }
}

const fn token_channel(channel: GenerationChannel) -> &'static str {
    match channel {
        GenerationChannel::Content => "content",
        GenerationChannel::Reasoning => "reasoning",
        GenerationChannel::ToolCalls => "tool_calls",
    }
}

fn send_completion(
    sender: &mpsc::Sender<Result<proto::GenerateEvent, Status>>,
    result: GenerationResult,
    model: &str,
    operation_id: &str,
) {
    let output = result.output;
    let completion = proto::Completion {
        reasoning: output.reasoning,
        tool_calls: libmir::ToolCall::parse_mistral(&output.tool_calls)
            .unwrap_or_default()
            .into_iter()
            .map(proto_tool_call)
            .collect(),
        prefill_tokens_per_second: output.metrics.throughput.prefill.per_second,
        decode_tokens_per_second: output.metrics.throughput.decode.per_second,
        prefill_ms: Some(output.metrics.durations_ms.prefill),
        decode_ms: Some(output.metrics.durations_ms.decode),
        text: output.text,
        prompt_tokens: u64::try_from(output.prompt_tokens).unwrap_or(u64::MAX),
        completion_tokens: u64::try_from(output.token_ids.len()).unwrap_or(u64::MAX),
        finish_reason: output.finish_reason.to_owned(),
        elapsed_ms: result.elapsed_ms,
        tokens_per_second: result.tokens_per_second,
        ttft_ms: result.ttft_ms,
    };
    tracing::info!(operation = operation_id, %model, prompt_tokens = completion.prompt_tokens,
        completion_tokens = completion.completion_tokens, finish_reason = %completion.finish_reason,
        elapsed_ms = completion.elapsed_ms, "generation completed");
    drop(sender.blocking_send(Ok(proto::GenerateEvent {
        event: Some(proto::generate_event::Event::Completion(completion)),
    })));
}

fn generation_error(error: crate::application::Error) -> Status {
    super::status::application(error)
}

fn proto_tool_call(call: libmir::ToolCall) -> proto::ChatToolCall {
    proto::ChatToolCall {
        id: call.id,
        r#type: call.kind,
        function: Some(proto::ChatFunctionCall {
            name: call.function.name,
            arguments_json: call.function.arguments.to_string(),
        }),
    }
}
