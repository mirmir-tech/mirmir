use std::time::Instant;

use libmir::{
    Engine,
    models::layout::{ModelLayout, ModelMetadata},
    runtime::{
        backend::{ModelHandle, PrefillOutput, SamplingLogits},
        kv::{KvCache, KvSessionState},
        metrics::GenerationMetricsRecorder,
    },
};

use super::{
    output::manifest,
    progress::{ProgressBar, ProgressUnit},
};
use crate::{args::ChatArgs, error::CliError};

pub(super) fn load_model(
    backend: &Engine,
    layout: &ModelLayout,
    metadata: &ModelMetadata,
    args: &ChatArgs,
    metrics: &mut GenerationMetricsRecorder,
) -> Result<ModelHandle, CliError> {
    let manifest = manifest(layout, metadata, args, backend.target());
    let started = Instant::now();
    let bar = ProgressBar::start("loading weights", weight_bytes(layout), ProgressUnit::Bytes);
    let mut progress = |event: libmir::ProgressEvent| bar.update(event.current, &event.detail);
    let handle = backend.load_model_with_progress(&manifest, &mut progress)?;
    bar.finish();
    metrics.record_load(started.elapsed());
    Ok(handle)
}

pub(super) struct PrefillStage<'a> {
    pub backend: &'a Engine,
    pub handle: &'a ModelHandle,
    pub prompt_tokens: &'a [u32],
    pub kv_cache: &'a mut KvCache,
    pub session: &'a mut KvSessionState,
    pub metrics: &'a mut GenerationMetricsRecorder,
    pub sampling_logits: SamplingLogits,
}

pub(super) fn prefill_prompt(input: PrefillStage<'_>) -> Result<PrefillOutput, CliError> {
    let PrefillStage {
        backend,
        handle,
        prompt_tokens,
        kv_cache,
        session,
        metrics,
        sampling_logits,
    } = input;
    let prefill = session.prepare_prefill_in_place(kv_cache, prompt_tokens)?;
    let started = Instant::now();
    let bar = ProgressBar::start(
        format!("prefill {} tokens", prompt_tokens.len()),
        u64::try_from(prompt_tokens.len())?,
        ProgressUnit::Tokens,
    );
    let mut progress = |event: libmir::ProgressEvent| bar.update(event.current, &event.detail);
    let output = backend.prefill_tokens_with_progress(
        handle,
        prefill.session_id,
        prompt_tokens,
        session.table(),
        sampling_logits,
        &mut progress,
    )?;
    bar.finish();
    metrics.record_prefill(started.elapsed(), output.accepted_tokens);
    let _committed = session.commit_ready_prefix_blocks(kv_cache)?;
    Ok(output)
}

fn weight_bytes(layout: &ModelLayout) -> u64 {
    layout.weights.iter().map(|weight| weight.bytes).sum()
}
