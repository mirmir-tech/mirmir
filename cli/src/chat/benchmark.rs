use std::time::Duration;

use libmir::{
    Engine, RuntimeConfig,
    models::{generation::GenerationSettings, layout::DecoderConfig, tokenizer::TextTokenizer},
    runtime::{
        backend::{Backend, ModelHandle, SamplingLogits},
        kv::{KvCache, KvSessionState},
        metrics::{GenerationMetrics, GenerationMetricsRecorder},
        sampling::Sampler,
    },
};
use uuid::Uuid;

use super::{
    generation::{self, TokenGenerationInput},
    prime::{PrimeStage, prime_prompt},
    progress::TokenStreamer,
    sampling::{sampler_config, sampling_backend_line, sampling_logits, sampling_vocab_limit},
};
use crate::{args::ChatArgs, error::CliError, ui};

pub(super) fn run(input: &BenchmarkInput<'_>) -> Result<Vec<String>, CliError> {
    let BenchmarkInput {
        backend,
        handle,
        args,
        generation,
        decoder,
        tokenizer,
        prompt_tokens,
        config,
    } = input;
    if args.bench_samples == 0 {
        return Err(CliError::Message("bench_samples must be greater than zero".into()));
    }
    let sampling_vocab = sampling_vocab_limit(tokenizer, decoder);
    let sampling_logits = sampling_logits(generation, sampling_vocab);
    let total = args.bench_warmup + args.bench_samples;
    let mut samples = Vec::with_capacity(args.bench_samples);
    for sample in 0..total {
        backend.clear_prefix_cache(handle)?;
        let metrics = run_sample(
            backend, handle, args, generation, tokenizer, prompt_tokens, config, sampling_vocab,
            sampling_logits,
        )?;
        if sample >= args.bench_warmup {
            samples.push(metrics);
        }
    }
    Ok(ui::production_bench_report(
        &handle.id,
        prompt_tokens.len(),
        generation.max_tokens,
        args.bench_warmup,
        &sampling_backend_line(generation, sampling_vocab, &backend.info()),
        &diagnostic_flags(),
        &samples,
    ))
}

pub(super) struct BenchmarkInput<'a> {
    pub backend: &'a Engine,
    pub handle: &'a ModelHandle,
    pub args: &'a ChatArgs,
    pub generation: &'a GenerationSettings,
    pub decoder: &'a DecoderConfig,
    pub tokenizer: &'a TextTokenizer,
    pub prompt_tokens: &'a [u32],
    pub config: &'a RuntimeConfig,
}

#[allow(clippy::too_many_arguments)]
fn run_sample(
    backend: &Engine,
    handle: &ModelHandle,
    args: &ChatArgs,
    generation: &GenerationSettings,
    tokenizer: &TextTokenizer,
    prompt_tokens: &[u32],
    config: &RuntimeConfig,
    sampling_vocab: usize,
    sampling_logits: SamplingLogits,
) -> Result<GenerationMetrics, CliError> {
    let mut metrics = GenerationMetricsRecorder::new();
    metrics.record_prompt(Duration::ZERO, prompt_tokens.len());
    let mut kv_cache = KvCache::with_config(config.kv_cache);
    let mut session = KvSessionState::new(Uuid::new_v4(), &handle.id, config.kv_cache.block_size);
    let mut sampler = Sampler::new(sampler_config(generation, args.seed, sampling_vocab))?;
    let prefill = prime_prompt(PrimeStage {
        backend,
        handle,
        prompt_tokens,
        kv_cache: &mut kv_cache,
        session: &mut session,
        metrics: &mut metrics,
        sampling_logits: super::sampling::request_sampling_logits(
            generation, sampling_vocab, &mut sampler,
        ),
    })?;
    let mut streamer = TokenStreamer::disabled();
    let mut generation = TokenGenerationInput {
        backend,
        handle,
        args,
        generation,
        text_tokenizer: tokenizer,
        sampler: &mut sampler,
        kv_cache: &mut kv_cache,
        session: &mut session,
        metrics: &mut metrics,
        prompt_tokens,
        prefill_token: prefill.next_token,
        prefill_logits: prefill.logits.as_ref(),
        prefill_candidates: prefill.candidates.as_ref(),
        sampling_logits,
        sampling_vocab,
        streamer: &mut streamer,
    };
    let _generated = backend.with_generation_scope(|| generation::generate(&mut generation))?;
    Ok(metrics.snapshot(kv_cache.stats()))
}

fn diagnostic_flags() -> String {
    const FLAGS: [&str; 2] = ["MIRMIR_METAL_PROFILE_LAYERS", "MIRMIR_METAL_PROFILE_COMPONENTS"];
    let active = FLAGS
        .into_iter()
        .filter_map(|name| {
            let value = std::env::var(name).ok()?;
            matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
                .then(|| format!("{name}={value}"))
        })
        .collect::<Vec<_>>();
    if active.is_empty() {
        return "no profile or paged-attention overrides".into();
    }
    format!("active overrides: {}", active.join(", "))
}
