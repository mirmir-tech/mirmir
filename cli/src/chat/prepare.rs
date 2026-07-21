use std::time::Instant;

use libmir::{
    models::{
        chat::{ChatPrompt, ChatTemplate},
        generation::{GenerationConfig, GenerationOverrides, GenerationSettings},
        layout::{DecoderConfig, ModelLayout, ModelMetadata},
        tokenizer::{TextTokenizer, TokenizedPrompt},
    },
    runtime::metrics::GenerationMetricsRecorder,
};

use super::{
    output::{chat_request, validate_context},
    progress::ProgressLine,
};
use crate::{args::ChatArgs, error::CliError};

pub(super) struct PreparedChat {
    pub layout: ModelLayout,
    pub metadata: ModelMetadata,
    pub decoder: DecoderConfig,
    pub generation: GenerationSettings,
    pub template: ChatTemplate,
    pub text_tokenizer: TextTokenizer,
    pub encoded: TokenizedPrompt,
    pub prompt: ChatPrompt,
}

pub(super) fn prepare_chat(
    args: &ChatArgs,
    metrics: &mut GenerationMetricsRecorder,
) -> Result<PreparedChat, CliError> {
    let inspect_started = Instant::now();
    let mut progress = ProgressLine::start("inspect model");
    let layout = ModelLayout::inspect(&args.model)?;
    let metadata = ModelMetadata::from_layout(&layout)?;
    let decoder = DecoderConfig::from_layout(&layout)?;
    let generation = GenerationConfig::from_layout(&layout)?.resolve(GenerationOverrides {
        max_tokens: args.max_tokens,
        temperature: args.temperature,
        top_p: args.top_p,
        top_k: args.top_k,
        repetition_penalty: args.repetition_penalty,
    })?;
    let template = ChatTemplate::from_layout(&layout)?;
    let text_tokenizer = TextTokenizer::from_layout(&layout)?;
    progress.finish()?;
    metrics.record_inspect(inspect_started.elapsed());

    let prompt_started = Instant::now();
    let request = chat_request(args, generation);
    let prompt = template.render(&request)?;
    let encoded =
        text_tokenizer.encode_with_special_tokens(&prompt.text, prompt.add_special_tokens)?;
    if encoded.token_ids.is_empty() {
        return Err(CliError::Message("tokenized prompt cannot be empty".into()));
    }
    validate_context(encoded.token_ids.len(), generation.max_tokens, metadata.context_len)?;
    metrics.record_prompt(prompt_started.elapsed(), encoded.token_ids.len());
    Ok(PreparedChat {
        layout,
        metadata,
        decoder,
        generation,
        template,
        text_tokenizer,
        encoded,
        prompt,
    })
}
