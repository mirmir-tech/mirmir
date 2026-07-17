use libmir::{
    Engine, RuntimeConfig,
    runtime::{
        backend::Backend,
        kv::{KvCache, KvSessionState},
        metrics::GenerationMetricsRecorder,
        sampling::Sampler,
    },
};
use uuid::Uuid;

use super::super::{
    benchmark::{self, BenchmarkInput},
    generation::{self, TokenGenerationInput},
    output::{HeaderInput, validate, write_lines},
    prepare::{PreparedChat, prepare_chat},
    prime::{PrimeStage, prime_prompt},
    progress::TokenStreamer,
    render::{RenderInput, render},
    report::collect_report_context,
    response::ChatResponse,
    sampling::{request_sampling_logits, sampler_config, sampling_logits, sampling_vocab_limit},
    speed,
    stage::load_model,
};
use crate::{args::ChatArgs, error::CliError};

pub fn run(config: &RuntimeConfig, args: ChatArgs) -> Result<(), CliError> {
    validate(&args)?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let output = runtime.block_on(run_async(config, args))?;
    write_lines(output.lines)?;
    if let Some(metrics) = output.metrics {
        speed::write(&metrics)?;
    }
    Ok(())
}

struct ChatOutput {
    lines: Vec<String>,
    metrics: Option<libmir::runtime::metrics::GenerationMetrics>,
}

async fn run_async(config: &RuntimeConfig, args: ChatArgs) -> Result<ChatOutput, CliError> {
    let mut metrics = GenerationMetricsRecorder::new();
    let prepared = prepare_chat(&args, &mut metrics)?;
    run_prepared(config, args, metrics, prepared).await
}

async fn run_prepared(
    config: &RuntimeConfig,
    args: ChatArgs,
    mut metrics: GenerationMetricsRecorder,
    prepared: PreparedChat,
) -> Result<ChatOutput, CliError> {
    let PreparedChat {
        layout,
        metadata,
        decoder,
        generation,
        template,
        text_tokenizer,
        encoded,
        prompt,
    } = prepared;
    let backend = Engine::from_config(config)?;
    backend.set_profile_decode(args.trace);
    let handle = load_model(&backend, &layout, &metadata, &args, &mut metrics)?;
    if args.bench {
        let lines = benchmark::run(&BenchmarkInput {
            backend: &backend,
            handle: &handle,
            args: &args,
            generation: &generation,
            decoder: &decoder,
            tokenizer: &text_tokenizer,
            prompt_tokens: &encoded.token_ids,
            config,
        })?;
        return Ok(ChatOutput { lines, metrics: None });
    }
    let header_input = HeaderInput {
        args: &args,
        layout: &layout,
        generation: &generation,
        metadata: &metadata,
        decoder: &decoder,
        backend: &backend.info(),
        text_tokenizer: &text_tokenizer,
        encoded: &encoded,
        template: &template,
        prompt: &prompt,
    };
    let (header, mut trace_lines) =
        collect_report_context(&args, &backend, &handle, &header_input).await?;
    let sampling_vocab = sampling_vocab_limit(&text_tokenizer, &decoder);
    let mut sampler = Sampler::new(sampler_config(&generation, args.seed, sampling_vocab))?;
    let mut kv_cache = KvCache::with_config(config.kv_cache);
    let mut session = KvSessionState::new(Uuid::new_v4(), &handle.id, config.kv_cache.block_size);
    let prime_output = prime_prompt(PrimeStage {
        backend: &backend,
        handle: &handle,
        prompt_tokens: &encoded.token_ids,
        kv_cache: &mut kv_cache,
        session: &mut session,
        metrics: &mut metrics,
        sampling_logits: request_sampling_logits(&generation, sampling_vocab, &mut sampler),
    })?;
    if args.trace
        && let Some(trace) = prime_output.trace
    {
        trace_lines.push(format!("prefill.trace: {trace}"));
    }
    let mut streamer = if args.trace {
        TokenStreamer::disabled()
    } else {
        TokenStreamer::terminal(&text_tokenizer, args.review)
    };
    let mut input = TokenGenerationInput {
        backend: &backend,
        handle: &handle,
        args: &args,
        generation: &generation,
        text_tokenizer: &text_tokenizer,
        sampler: &mut sampler,
        kv_cache: &mut kv_cache,
        session: &mut session,
        metrics: &mut metrics,
        prompt_tokens: &encoded.token_ids,
        prefill_token: prime_output.next_token,
        prefill_logits: prime_output.logits.as_ref(),
        prefill_candidates: prime_output.candidates.as_ref(),
        sampling_logits: sampling_logits(&generation, sampling_vocab),
        sampling_vocab,
        streamer: &mut streamer,
    };
    let generated = generation::generate(&mut input)?;
    let did_stream = streamer.is_enabled();
    streamer.finish()?;
    if args.verbose || args.trace {
        trace_lines.extend(generated.traces.iter().cloned());
    }
    let response = ChatResponse::from_tokens(&text_tokenizer, &generated.tokens)?;
    Ok(chat_output(RenderInput {
        args: &args,
        did_stream,
        generated: &generated,
        response: &response,
        metrics: &metrics,
        stats: kv_cache.stats(),
        header: &header,
        trace: &trace_lines,
    }))
}

fn chat_output(input: RenderInput<'_>) -> ChatOutput {
    let metrics = input.metrics.snapshot(input.stats.clone());
    ChatOutput {
        lines: render(input),
        metrics: Some(metrics),
    }
}
