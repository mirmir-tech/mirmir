use std::time::Instant;

use libmir::{
    Engine,
    models::{generation::GenerationSettings, tokenizer::TextTokenizer},
    runtime::{
        backend::{CandidateLogitsTrace, LogitsTrace, SamplingLogits},
        kv::{KvCache, KvSessionState},
        metrics::GenerationMetricsRecorder,
        sampling::Sampler,
    },
};

use super::super::{
    cache::clear_after_emit, output::sample, progress::TokenStreamer,
    sampling::request_sampling_logits,
};
use crate::{args::ChatArgs, error::CliError};

pub(in crate::chat) struct GeneratedTokens {
    pub tokens: Vec<u32>,
    pub finish_reason: &'static str,
    pub traces: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub(in crate::chat) fn generate_tokens(
    backend: &Engine,
    handle: &libmir::runtime::backend::ModelHandle,
    args: &ChatArgs,
    generation: &GenerationSettings,
    text_tokenizer: &TextTokenizer,
    sampler: &mut Sampler,
    kv_cache: &mut KvCache,
    session: &mut KvSessionState,
    metrics: &mut GenerationMetricsRecorder,
    prompt_tokens: &[u32],
    prefill_token: Option<u32>,
    prefill_logits: Option<&LogitsTrace>,
    prefill_candidates: Option<&CandidateLogitsTrace>,
    sampling_logits: SamplingLogits,
    sampling_vocab: usize,
    streamer: &mut TokenStreamer,
) -> Result<GeneratedTokens, CliError> {
    let stop_ids = text_tokenizer.stop_token_ids();
    let mut tokens = Vec::with_capacity(generation.max_tokens);
    let mut traces = Vec::new();
    let mut history = sampling_logits.requires_history().then(|| prompt_tokens.to_vec());
    let sample_started = Instant::now();
    let mut next = choose_next(
        prefill_token,
        sampler,
        prefill_logits,
        prefill_candidates,
        history_slice(history.as_deref()),
    )?;
    metrics.record_sampling(sample_started.elapsed());
    let mut finish_reason = "max_tokens";
    while tokens.len() < generation.max_tokens {
        tokens.push(next);
        if let Some(history) = history.as_mut() {
            history.push(next);
        }
        streamer.write_token(text_tokenizer, next)?;
        clear_after_emit(backend, tokens.len())?;
        if stop_ids.contains(&next) {
            finish_reason = "stop";
            break;
        }
        if tokens.len() == generation.max_tokens {
            break;
        }
        let decode = session.append_decode_in_place(kv_cache, next)?;
        let decode_started = Instant::now();
        let output = backend.decode_token(
            handle,
            decode.session_id,
            next,
            session.table(),
            request_sampling_logits(generation, sampling_vocab, sampler),
        )?;
        collect_decode_trace(args, tokens.len(), &output.event.text, &mut traces);
        metrics.record_decode(decode_started.elapsed());
        let _committed = session.commit_ready_prefix_blocks(kv_cache)?;
        let sample_started = Instant::now();
        next = choose_next(
            output.event.token_id,
            sampler,
            output.logits.as_ref(),
            output.candidates.as_ref(),
            history_slice(history.as_deref()),
        )?;
        metrics.record_sampling(sample_started.elapsed());
    }
    metrics.record_generated(tokens.len());
    Ok(GeneratedTokens { tokens, finish_reason, traces })
}

fn collect_decode_trace(args: &ChatArgs, step: usize, trace: &str, traces: &mut Vec<String>) {
    if !(args.verbose || args.trace) || !trace.contains("decode.stage_profile") || traces.len() >= 4
    {
        return;
    }
    traces.push(format!("decode.step_{step}: {trace}"));
}

fn choose_next(
    token_id: Option<u32>,
    sampler: &mut Sampler,
    logits: Option<&LogitsTrace>,
    candidates: Option<&CandidateLogitsTrace>,
    history: &[u32],
) -> Result<u32, CliError> {
    token_id.map_or_else(|| sample(sampler, logits, candidates, history), Ok)
}

fn history_slice(history: Option<&[u32]>) -> &[u32] {
    history.unwrap_or(&[])
}
