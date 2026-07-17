use libmir::{
    Engine,
    models::{generation::GenerationSettings, tokenizer::TextTokenizer},
    runtime::{
        backend::{CandidateLogitsTrace, LogitsTrace, ModelHandle, SamplingLogits},
        kv::{KvCache, KvSessionState},
        metrics::GenerationMetricsRecorder,
        sampling::Sampler,
    },
};

use super::{
    flow::{GeneratedTokens, generate_tokens},
    progress::TokenStreamer,
};
use crate::{args::ChatArgs, error::CliError};

pub(super) struct TokenGenerationInput<'a> {
    pub backend: &'a Engine,
    pub handle: &'a ModelHandle,
    pub args: &'a ChatArgs,
    pub generation: &'a GenerationSettings,
    pub text_tokenizer: &'a TextTokenizer,
    pub sampler: &'a mut Sampler,
    pub kv_cache: &'a mut KvCache,
    pub session: &'a mut KvSessionState,
    pub metrics: &'a mut GenerationMetricsRecorder,
    pub prompt_tokens: &'a [u32],
    pub prefill_token: Option<u32>,
    pub prefill_logits: Option<&'a LogitsTrace>,
    pub prefill_candidates: Option<&'a CandidateLogitsTrace>,
    pub sampling_logits: SamplingLogits,
    pub sampling_vocab: usize,
    pub streamer: &'a mut TokenStreamer,
}

pub(super) fn generate(input: &mut TokenGenerationInput<'_>) -> Result<GeneratedTokens, CliError> {
    let backend = input.backend;
    backend.with_generation_scope(|| {
        generate_tokens(
            input.backend,
            input.handle,
            input.args,
            input.generation,
            input.text_tokenizer,
            &mut *input.sampler,
            &mut *input.kv_cache,
            &mut *input.session,
            &mut *input.metrics,
            input.prompt_tokens,
            input.prefill_token,
            input.prefill_logits,
            input.prefill_candidates,
            input.sampling_logits,
            input.sampling_vocab,
            &mut *input.streamer,
        )
    })
}
