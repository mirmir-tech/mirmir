use std::time::Instant;

use libmir::{
    CancellationToken, GenerationOutput, GenerationRequest, GenerationToken, ProgressEvent,
    ProgressStage,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::super::{
    ActivityKind, ActivityOutcome, ActivityProgress, ActivityStage, Application, CompletionMetrics,
    GenerationTelemetry, Operation, Result,
};

pub struct GenerationSession {
    selector: String,
    operation: Operation,
    cancellation: CancellationToken,
    telemetry: GenerationTelemetry,
    started: Instant,
}

pub struct GenerationResult {
    pub output: GenerationOutput,
    pub elapsed_ms: f64,
    pub ttft_ms: Option<f64>,
    pub tokens_per_second: Option<f64>,
}

pub enum GenerationEvent {
    Started { operation_id: String },
    Token(GenerationToken),
    Completion(Box<GenerationResult>),
}

impl Application {
    pub fn start_generation(&self, selector: &str) -> GenerationSession {
        let cancellation = CancellationToken::new();
        GenerationSession {
            selector: selector.to_owned(),
            operation: self.activity.begin(
                ActivityKind::Generate,
                selector,
                Some(cancellation.clone()),
            ),
            cancellation,
            telemetry: self.telemetry.begin(),
            started: Instant::now(),
        }
    }

    pub fn generation_stream(
        &self,
        selector: &str,
        request: GenerationRequest,
        image: Option<Vec<u8>>,
    ) -> ReceiverStream<Result<GenerationEvent>> {
        let application = self.clone();
        let mut session = self.start_generation(selector);
        let (sender, receiver) = mpsc::channel(64);
        drop(sender.try_send(Ok(GenerationEvent::Started {
            operation_id: session.operation_id().to_owned(),
        })));
        drop(tokio::task::spawn_blocking(move || {
            let cancellation = session.cancellation();
            let tokens = sender.clone();
            let mut progress = |_event| {};
            let mut token = move |token| {
                if tokens.blocking_send(Ok(GenerationEvent::Token(token))).is_err() {
                    cancellation.cancel();
                }
            };
            let result = application.generate(
                &mut session,
                &request,
                image.as_deref(),
                &mut progress,
                &mut token,
            );
            drop(match result {
                Ok(result) => {
                    sender.blocking_send(Ok(GenerationEvent::Completion(Box::new(result))))
                },
                Err(error) => sender.blocking_send(Err(error)),
            });
        }));
        ReceiverStream::new(receiver)
    }

    pub fn generate(
        &self,
        session: &mut GenerationSession,
        request: &GenerationRequest,
        image: Option<&[u8]>,
        progress: &mut dyn FnMut(ProgressEvent),
        token: &mut dyn FnMut(GenerationToken),
    ) -> Result<GenerationResult> {
        session.telemetry.resolving();
        session.operation.progress(ActivityStage::Resolving, "resolving model", None);
        let telemetry = &session.telemetry;
        let operation = session.operation.clone();
        let selector = session.selector.clone();
        let mut loading = |event: ProgressEvent| {
            telemetry.progress(&event);
            operation.progress(
                ActivityStage::Runtime(event.stage()),
                event.detail(),
                Some(activity_progress(&event)),
            );
            tracing::debug!(model = %selector, stage = ?event.stage(), current = event.count().current(), total = event.count().total(), "model progress");
        };
        let model = self.runtime.load_model(&session.selector, false, &mut loading)?.model;
        session.telemetry.stage(ProgressStage::PrefillTokens);
        session.operation.progress(
            ActivityStage::Runtime(ProgressStage::PrefillTokens),
            "prefilling prompt",
            None,
        );
        let mut ttft_ms = None;
        let telemetry = &session.telemetry;
        let operation = session.operation.clone();
        let started = session.started;
        let mut tracked_progress = |event: ProgressEvent| {
            telemetry.progress(&event);
            update_progress(&operation, &event);
            progress(event);
        };
        let mut tracked_token = |value: GenerationToken| {
            telemetry.token_emitted();
            ttft_ms.get_or_insert_with(|| started.elapsed().as_secs_f64() * 1_000.0);
            token(value);
        };
        let result = match image {
            Some(image) => model.generate_image_cancellable(
                request,
                image,
                &mut tracked_progress,
                &mut tracked_token,
                &session.cancellation,
            ),
            None => model.generate_cancellable(
                request,
                &mut tracked_progress,
                &mut tracked_token,
                &session.cancellation,
            ),
        };
        match result {
            Ok(output) => Ok(session.complete(output, ttft_ms)),
            Err(error) => {
                session.fail(&error);
                Err(error.into())
            },
        }
    }
}

impl GenerationSession {
    #[must_use]
    pub fn operation_id(&self) -> &str {
        self.operation.id()
    }

    pub fn reject(&mut self, detail: &str) {
        self.cancellation.cancel();
        self.telemetry.fail();
        self.operation.finish(ActivityOutcome::Failed, detail);
    }

    #[must_use]
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    fn complete(&mut self, output: GenerationOutput, ttft_ms: Option<f64>) -> GenerationResult {
        let elapsed = self.started.elapsed().as_secs_f64();
        let completion_tokens = u64::try_from(output.token_ids.len()).unwrap_or(u64::MAX);
        let tokens_per_second = completion_tokens
            .to_string()
            .parse::<f64>()
            .ok()
            .filter(|_| elapsed > 0.0)
            .map(|tokens| tokens / elapsed);
        self.telemetry.complete(&CompletionMetrics {
            prompt_tokens: u64::try_from(output.prompt_tokens).unwrap_or(u64::MAX),
            completion_tokens,
            tokens_per_second,
            ttft_ms,
            prefill_tokens_per_second: output.metrics.throughput.prefill.per_second,
            decode_tokens_per_second: output.metrics.throughput.decode.per_second,
        });
        self.operation.finish(ActivityOutcome::Completed, "generation completed");
        GenerationResult {
            output,
            elapsed_ms: elapsed * 1_000.0,
            ttft_ms,
            tokens_per_second,
        }
    }

    fn fail(&mut self, error: &libmir::Error) {
        self.telemetry.fail();
        let state = if matches!(error, libmir::Error::Cancelled) {
            ActivityOutcome::Cancelled
        } else {
            ActivityOutcome::Failed
        };
        self.operation.finish(state, &error.to_string());
    }
}

fn update_progress(operation: &Operation, event: &ProgressEvent) {
    let count = event.count();
    if event.stage() != ProgressStage::DecodeTokens
        || count.current() == 0
        || count.current() == count.total()
        || count.current().is_multiple_of(16)
    {
        operation.progress(
            ActivityStage::Runtime(event.stage()),
            event.detail(),
            Some(activity_progress(event)),
        );
    }
}

const fn activity_progress(event: &ProgressEvent) -> ActivityProgress {
    let count = event.count();
    ActivityProgress::new(count.current(), Some(count.total()))
}
