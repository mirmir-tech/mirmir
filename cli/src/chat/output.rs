use std::{
    ffi::OsStr,
    io::{self, Write},
    path::Path,
};

use libmir::{
    foundation::{
        model::{BackendTarget, ModelManifest},
        protocol::{ChatCompletionRequest, ChatMessage},
    },
    models::{
        chat::ChatTemplate,
        generation::GenerationSettings,
        layout::{DecoderConfig, ModelLayout, ModelMetadata},
        tokenizer::{TextTokenizer, TokenizedPrompt},
    },
    runtime::{
        backend::{BackendInfo, CandidateLogitsTrace, LogitsTrace},
        sampling::Sampler,
    },
};

use super::sampling::{sampling_backend_line, sampling_vocab_limit};
use crate::{args::ChatArgs, error::CliError};

pub(super) fn validate(args: &ChatArgs) -> Result<(), CliError> {
    if args.prompt.trim().is_empty() {
        return Err(CliError::Message("prompt cannot be empty".into()));
    }
    Ok(())
}

pub(super) fn validate_context(
    prompt_tokens: usize,
    max_tokens: usize,
    context_len: usize,
) -> Result<(), CliError> {
    let requested = prompt_tokens + max_tokens;
    if requested > context_len {
        return Err(CliError::Message(format!(
            "requested {requested} tokens exceeds model context {context_len} (prompt {prompt_tokens}, max_tokens {max_tokens})"
        )));
    }
    Ok(())
}

pub(super) struct HeaderInput<'a> {
    pub args: &'a ChatArgs,
    pub layout: &'a ModelLayout,
    pub generation: &'a GenerationSettings,
    pub metadata: &'a ModelMetadata,
    pub decoder: &'a DecoderConfig,
    pub backend: &'a BackendInfo,
    pub text_tokenizer: &'a TextTokenizer,
    pub encoded: &'a TokenizedPrompt,
    pub template: &'a ChatTemplate,
    pub prompt: &'a libmir::models::chat::ChatPrompt,
}

pub(super) fn chat_request(
    args: &ChatArgs,
    generation: GenerationSettings,
) -> ChatCompletionRequest {
    ChatCompletionRequest {
        model: model_id(&args.model),
        messages: vec![ChatMessage {
            role: "user".into(),
            content: args.prompt.clone(),
        }],
        stream: false,
        max_tokens: Some(generation.max_tokens),
        temperature: Some(generation.temperature),
        top_p: Some(generation.top_p),
        top_k: Some(generation.top_k),
        repetition_penalty: Some(generation.repetition_penalty),
        seed: args.seed,
    }
}

pub(super) fn manifest(
    layout: &ModelLayout,
    metadata: &ModelMetadata,
    args: &ChatArgs,
    target: BackendTarget,
) -> ModelManifest {
    ModelManifest {
        id: model_id(&args.model),
        path: layout.root.display().to_string(),
        tokenizer_path: layout.tokenizer_path.as_ref().map(|path| path.display().to_string()),
        context_len: metadata.context_len,
        quantization: metadata.quantization.clone(),
        preferred_backends: vec![target],
    }
}

pub(super) fn header_lines(input: &HeaderInput<'_>) -> Vec<String> {
    vec![
        format!("model: {}", model_id(&input.args.model)),
        format!("backend: {} on {}", input.backend.name, input.backend.device),
        format!("prompt_template: {:?} ({:?})", input.template.kind(), input.prompt.source),
        format!(
            "prompt_tokens: {}, prompt_bytes: {}",
            input.encoded.token_ids.len(),
            input.encoded.bytes
        ),
        format!("prompt_token_max: {}", prompt_token_max(input.encoded)),
        tokenizer_assets(input.layout),
        format!("tokenizer_vocab: {}", input.text_tokenizer.vocab_size()),
        vocab_line(input.text_tokenizer, input.decoder),
        format!(
            "generation: max_tokens {} | temperature {}, top_p {}, top_k {}, repetition_penalty {}",
            input.generation.max_tokens,
            input.generation.temperature,
            input.generation.top_p,
            input.generation.top_k,
            input.generation.repetition_penalty
        ),
        sampling_backend_line(
            input.generation,
            sampling_vocab_limit(input.text_tokenizer, input.decoder),
            input.backend,
        ),
        format!("stop_token_ids: {}", stop_ids(input.text_tokenizer)),
    ]
}

fn tokenizer_assets(layout: &ModelLayout) -> String {
    let source = layout
        .tokenizer_path
        .as_deref()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or("missing");
    let companions = [
        layout.tokenizer_config_path.as_ref().map(|_| "tokenizer_config.json"),
        layout.added_tokens_path.as_ref().map(|_| "added_tokens.json"),
        layout.special_tokens_map_path.as_ref().map(|_| "special_tokens_map.json"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let has_fallback = layout.vocab_path.as_ref().zip(layout.merges_path.as_ref()).is_some();
    let fallback = if has_fallback {
        "; fallback vocab.json + merges.txt available"
    } else {
        ""
    };
    format!(
        "tokenizer_assets: source {source}; companions {}{fallback}",
        companions.join(", ")
    )
}

pub(super) fn sample(
    sampler: &mut Sampler,
    logits: Option<&LogitsTrace>,
    candidates: Option<&CandidateLogitsTrace>,
    history: &[u32],
) -> Result<u32, CliError> {
    if let Some(candidates) = candidates {
        return Ok(sampler.sample_candidates_with_history(candidates, history)?);
    }
    let logits = logits.ok_or_else(|| CliError::Message("backend did not return logits".into()))?;
    Ok(sampler.sample_with_history(logits, history)?)
}

pub(super) fn write_lines(lines: impl IntoIterator<Item = String>) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for line in lines {
        writeln!(handle, "{line}")?;
    }
    Ok(())
}

fn vocab_line(text_tokenizer: &TextTokenizer, decoder: &DecoderConfig) -> String {
    format!(
        "vocab: config {}, tokenizer {}, sampling_limit {}",
        decoder.vocab_size,
        text_tokenizer.vocab_size(),
        sampling_vocab_limit(text_tokenizer, decoder)
    )
}

fn prompt_token_max(encoded: &TokenizedPrompt) -> String {
    encoded
        .token_ids
        .iter()
        .max()
        .map_or_else(|| "none".into(), ToString::to_string)
}

fn stop_ids(text_tokenizer: &TextTokenizer) -> String {
    let ids = text_tokenizer.stop_token_ids();
    if ids.is_empty() {
        return "none".into();
    }
    ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
}

fn model_id(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| "model".into(), |name| name.to_string_lossy().into_owned())
}
