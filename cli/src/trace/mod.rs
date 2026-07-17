use std::path::Path;

use libmir::{
    Engine, RuntimeConfig,
    foundation::model::{BackendTarget, ModelFamily, ModelManifest, Quantization},
    runtime::{
        backend::{
            Backend, DecodeRequest, LogitsTrace, ModelHandle, PrefillRequest, SamplingLogits,
        },
        kv::{BlockId, BlockTable},
    },
};
use uuid::Uuid;

use crate::{args::TraceMetalArgs, error::CliError};

mod input;

pub fn trace_mlx_model(
    config: &RuntimeConfig,
    args: &TraceMetalArgs,
) -> Result<Vec<String>, CliError> {
    let tokens = input::trace_tokens(args)?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(trace_mlx_model_async(config, args, tokens))
}

async fn trace_mlx_model_async(
    config: &RuntimeConfig,
    args: &TraceMetalArgs,
    tokens: Vec<u32>,
) -> Result<Vec<String>, CliError> {
    let backend = Engine::from_config(config)?;
    let manifest = manifest(&args.path, &args.model_id);
    let handle = backend.load_model(&manifest).await?;
    let trace = backend.model_trace(&handle).await?;
    let mut lines = trace.summary_lines();
    lines.insert(0, format!("loaded: {} on {}", handle.id, handle.backend));
    lines.push(input::token_summary(&tokens));
    let session_id = Uuid::new_v4();
    let prefill_len = tokens.len().saturating_sub(1).max(1);
    let prefill_tokens = tokens[..prefill_len].to_vec();
    let prefill_table = block_table(prefill_tokens.len(), config.kv_cache.block_size)?;
    let prefill = backend
        .prefill(PrefillRequest {
            model: handle.clone(),
            session_id,
            prompt_tokens: prefill_tokens,
            block_table: prefill_table,
            sampling_logits: SamplingLogits::Full,
        })
        .await?;
    lines.push(format!("prefill.accepted_tokens: {}", prefill.accepted_tokens));
    if let Some(logits) = prefill.logits.as_ref() {
        lines.push(format!("prefill.logits: {}", logits_summary(logits)));
    }
    if let Some(trace) = prefill.trace {
        lines.push(format!("prefill.trace: {trace}"));
    }
    if let Some(token_id) = tokens.get(prefill_len).copied() {
        let decode_table = block_table(prefill_len + 1, config.kv_cache.block_size)?;
        let decode = backend
            .decode(DecodeRequest {
                model: handle.clone(),
                session_id,
                token_id,
                block_table: decode_table,
                sampling_logits: SamplingLogits::Full,
            })
            .await?;
        lines.push(format!("decode.trace: {}", decode.event.text));
        if let Some(decode_logits) = decode.logits {
            lines.push(format!("decode.logits: {}", logits_summary(&decode_logits)));
            let full_logits =
                full_prefill_logits(&backend, &handle, config, &tokens[..=prefill_len]).await?;
            let full_last = last_logits(&full_logits)?;
            let decode_last = last_logits(&decode_logits)?;
            let diff = logit_diff(full_last, decode_last)?;
            lines.push(format!(
                "logits.prefill_decode_shape: {:?} vs {:?}",
                full_logits.shape, decode_logits.shape
            ));
            lines.push(format!("logits.full_top1: {}", top_token(full_last)));
            lines.push(format!("logits.decode_top1: {}", top_token(decode_last)));
            lines.extend(diff.lines(args.max_diff));
        } else {
            lines.push("logits.prefill_decode_max_abs_diff: unavailable".into());
        }
    }
    Ok(lines)
}

fn top_token(logits: &[f32]) -> String {
    logits
        .iter()
        .copied()
        .enumerate()
        .filter(|(_index, score)| score.is_finite())
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .map_or_else(|| "none".into(), |(index, score)| format!("id {index}, logit {score:.6e}"))
}

async fn full_prefill_logits(
    backend: &Engine,
    handle: &ModelHandle,
    config: &RuntimeConfig,
    tokens: &[u32],
) -> Result<LogitsTrace, CliError> {
    let output = backend
        .prefill(PrefillRequest {
            model: handle.clone(),
            session_id: Uuid::new_v4(),
            prompt_tokens: tokens.to_vec(),
            block_table: block_table(tokens.len(), config.kv_cache.block_size)?,
            sampling_logits: SamplingLogits::Full,
        })
        .await?;
    output
        .logits
        .ok_or_else(|| CliError::Message("full prefill did not produce logits".into()))
}

fn manifest(path: &Path, model_id: &str) -> ModelManifest {
    ModelManifest {
        id: model_id.to_owned(),
        family: ModelFamily::Unknown,
        path: path.display().to_string(),
        tokenizer_path: None,
        context_len: 0,
        quantization: Quantization::None,
        preferred_backends: vec![BackendTarget::Metal],
    }
}

fn block_table(tokens: usize, block_size: usize) -> Result<BlockTable, CliError> {
    let block_size = block_size.max(1);
    let mut table = BlockTable::with_block_size(block_size);
    for id in 0..tokens.div_ceil(block_size) {
        table.push(BlockId(u32::try_from(id)?));
    }
    table.set_token_len(tokens);
    Ok(table)
}

fn logits_summary(logits: &LogitsTrace) -> String {
    let range = logits
        .finite_min_max()
        .map_or_else(|| "none".into(), |(min, max)| format!("{min:.6e}..{max:.6e}"));
    format!(
        "shape {:?}, finite {}, non_finite {}, finite_range {}",
        logits.shape,
        logits.finite_count(),
        logits.non_finite_count(),
        range
    )
}

fn last_logits(trace: &LogitsTrace) -> Result<&[f32], CliError> {
    if trace.shape.len() != 3 || trace.shape[0] != 1 || trace.shape[1] < 1 {
        return Err(CliError::Message(format!(
            "logits trace must be [1, S, V], got {:?}",
            trace.shape
        )));
    }
    let seq = usize::try_from(trace.shape[1])?;
    let vocab = usize::try_from(trace.shape[2])?;
    let start = (seq - 1) * vocab;
    let end = start + vocab;
    trace
        .values
        .get(start..end)
        .ok_or_else(|| CliError::Message("logits trace is truncated".into()))
}

fn logit_diff(left: &[f32], right: &[f32]) -> Result<LogitDiff, CliError> {
    if left.len() != right.len() {
        return Err(CliError::Message(format!(
            "logit lengths differ: {} != {}",
            left.len(),
            right.len()
        )));
    }
    let mut max_abs = 0.0_f32;
    let mut sum_abs = 0.0_f64;
    let mut sum_sq = 0.0_f64;
    for (left, right) in left.iter().zip(right) {
        let delta = (left - right).abs();
        max_abs = max_abs.max(delta);
        sum_abs += f64::from(delta);
        sum_sq += f64::from(delta * delta);
    }
    let len = left.len().to_string().parse::<f64>().unwrap_or(1.0);
    Ok(LogitDiff {
        max_abs,
        mean_abs: sum_abs / len,
        rms: (sum_sq / len).sqrt(),
    })
}

struct LogitDiff {
    max_abs: f32,
    mean_abs: f64,
    rms: f64,
}

impl LogitDiff {
    fn lines(&self, max_diff: f32) -> Vec<String> {
        vec![
            format!("logits.prefill_decode_max_abs_diff: {:.6e}", self.max_abs),
            format!("logits.prefill_decode_mean_abs_diff: {:.6e}", self.mean_abs),
            format!("logits.prefill_decode_rms_diff: {:.6e}", self.rms),
            format!(
                "logits.prefill_decode_status: {} (threshold {:.6e})",
                if self.max_abs <= max_diff {
                    "pass"
                } else {
                    "fail"
                },
                max_diff
            ),
        ]
    }
}
