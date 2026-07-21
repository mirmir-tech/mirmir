use libmir::{
    foundation::protocol::{ChatCompletionRequest, ChatMessage},
    models::{
        chat::ChatTemplate,
        layout::ModelLayout,
        tokenizer::TextTokenizer,
    },
};

use crate::{args::TraceMetalArgs, error::CliError};

pub(super) fn trace_tokens(args: &TraceMetalArgs) -> Result<Vec<u32>, CliError> {
    args.prompt
        .as_deref()
        .map_or_else(|| parse_tokens(&args.tokens), |prompt| prompt_tokens(args, prompt))
}

pub(super) fn token_summary(tokens: &[u32]) -> String {
    let max = tokens.iter().max().map_or_else(|| "none".into(), ToString::to_string);
    format!("tokens: count {}, max {}", tokens.len(), max)
}

fn prompt_tokens(args: &TraceMetalArgs, prompt: &str) -> Result<Vec<u32>, CliError> {
    if prompt.trim().is_empty() {
        return Err(CliError::Message("prompt cannot be empty".into()));
    }
    let layout = ModelLayout::inspect(&args.path)?;
    let template = ChatTemplate::from_layout(&layout)?;
    let tokenizer = TextTokenizer::from_layout(&layout)?;
    let request = ChatCompletionRequest {
        model: args.model_id.clone(),
        messages: vec![ChatMessage {
            role: "user".into(),
            content: prompt.to_owned(),
        }],
        stream: false,
        max_tokens: Some(1),
        temperature: Some(0.0),
        top_p: Some(1.0),
        top_k: Some(0),
        repetition_penalty: Some(1.0),
        seed: None,
    };
    let rendered = template.render(&request)?;
    let encoded =
        tokenizer.encode_with_special_tokens(&rendered.text, rendered.add_special_tokens)?;
    ensure_pair(&encoded.token_ids)?;
    Ok(encoded.token_ids)
}

fn parse_tokens(raw_tokens: &str) -> Result<Vec<u32>, CliError> {
    let tokens: Result<Vec<_>, _> = raw_tokens
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::parse::<u32>)
        .collect();
    let tokens = tokens?;
    ensure_pair(&tokens)?;
    Ok(tokens)
}

fn ensure_pair(tokens: &[u32]) -> Result<(), CliError> {
    if tokens.len() < 2 {
        return Err(CliError::Message(
            "trace-metal needs at least two tokens for prefill/decode comparison".into(),
        ));
    }
    Ok(())
}
