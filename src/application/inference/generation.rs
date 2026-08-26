use std::time::Instant;

use libmir::{
    CancellationToken, ChatCompletionRequest, GenerationOutput, GenerationToken, ProgressEvent,
    ProgressStage,
};

use super::super::{Application, CompletionMetrics, GenerationTelemetry, Operation, Result};

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

impl Application {
    pub fn start_generation(&self, selector: &str) -> GenerationSession {
        let cancellation = CancellationToken::new();
        GenerationSession {
            selector: selector.to_owned(),
            operation: self.activity.begin("generate", selector, Some(cancellation.clone())),
            cancellation,
            telemetry: self.telemetry.begin(),
            started: Instant::now(),
        }
    }

    pub fn generate(
        &self,
        session: &mut GenerationSession,
        chat: &ChatCompletionRequest,
        image: Option<&[u8]>,
        progress: &mut dyn FnMut(ProgressEvent),
        token: &mut dyn FnMut(GenerationToken),
    ) -> Result<GenerationResult> {
        session.telemetry.stage("resolving");
        session.operation.progress("resolving", "resolving model", None, None);
        let telemetry = &session.telemetry;
        let operation = session.operation.clone();
        let selector = session.selector.clone();
        let mut loading = |event: ProgressEvent| {
            telemetry.progress(&event);
            let stage = match event.stage {
                ProgressStage::LoadWeights => "loading",
                ProgressStage::PrefillTokens => "warming",
                ProgressStage::DecodeTokens => "decoding",
            };
            operation.progress(stage, &event.detail, Some(event.current), Some(event.total));
            tracing::debug!(model = %selector, ?event.stage, current = event.current, total = event.total, "model progress");
        };
        let model = self.runtime.load_model(&session.selector, false, &mut loading)?.model;
        session.telemetry.stage("prefill");
        session.operation.progress("prefill", "prefilling prompt", None, None);
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
                chat,
                image,
                &mut tracked_progress,
                &mut tracked_token,
                &session.cancellation,
            ),
            None => model.generate_cancellable(
                chat,
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
        self.operation.finish("failed", detail);
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
        self.operation.finish("completed", "generation completed");
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
            "cancelled"
        } else {
            "failed"
        };
        self.operation.finish(state, &error.to_string());
    }
}

fn update_progress(operation: &Operation, event: &ProgressEvent) {
    let stage = match event.stage {
        ProgressStage::LoadWeights => "loading",
        ProgressStage::PrefillTokens => "prefill",
        ProgressStage::DecodeTokens => "decode",
    };
    if event.stage != ProgressStage::DecodeTokens
        || event.current == 0
        || event.current == event.total
        || event.current.is_multiple_of(16)
    {
        operation.progress(stage, &event.detail, Some(event.current), Some(event.total));
    }
}
