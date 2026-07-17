use libmir::{
    Engine,
    runtime::{
        backend::{CandidateLogitsTrace, LogitsTrace, ModelHandle, SamplingLogits},
        kv::{KvCache, KvSessionState},
        metrics::GenerationMetricsRecorder,
    },
};

use super::stage::{PrefillStage, prefill_prompt};
use crate::error::CliError;

pub(super) struct PrimeStage<'a> {
    pub backend: &'a Engine,
    pub handle: &'a ModelHandle,
    pub prompt_tokens: &'a [u32],
    pub kv_cache: &'a mut KvCache,
    pub session: &'a mut KvSessionState,
    pub metrics: &'a mut GenerationMetricsRecorder,
    pub sampling_logits: SamplingLogits,
}

pub(super) struct PrimeOutput {
    pub next_token: Option<u32>,
    pub logits: Option<LogitsTrace>,
    pub candidates: Option<CandidateLogitsTrace>,
    pub trace: Option<String>,
}

pub(super) fn prime_prompt(input: PrimeStage<'_>) -> Result<PrimeOutput, CliError> {
    let PrimeStage {
        backend,
        handle,
        prompt_tokens,
        kv_cache,
        session,
        metrics,
        sampling_logits,
    } = input;
    let output = prefill_prompt(PrefillStage {
        backend,
        handle,
        prompt_tokens,
        kv_cache,
        session,
        metrics,
        sampling_logits,
    })?;
    Ok(PrimeOutput {
        next_token: output.next_token,
        logits: output.logits,
        candidates: output.candidates,
        trace: output.trace,
    })
}
