use libmir::ReasoningMode;
use serde::Deserialize;

use super::error::ApiError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateOptions {
    pub enable_thinking: Option<bool>,
    /// Shorter reasoning, for templates that declare `medium_effort`.
    pub medium_effort: Option<bool>,
    /// Drops earlier turns' reasoning, for templates that declare
    /// `truncate_history_thinking`.
    pub truncate_history_thinking: Option<bool>,
}

/// Accept the vLLM thinking switch without silently changing a caller's
/// declared mode. Unsupported template options fail during deserialization.
pub(super) fn resolve(
    explicit: Option<ReasoningMode>,
    template: Option<&TemplateOptions>,
) -> Result<ReasoningMode, ApiError> {
    let compatible = template.and_then(|options| options.enable_thinking).map(|enabled| {
        if enabled {
            ReasoningMode::Enabled
        } else {
            ReasoningMode::Disabled
        }
    });
    match (explicit, compatible) {
        (Some(left), Some(right)) if left != right => Err(ApiError::bad_request(
            "reasoning and chat_template_kwargs.enable_thinking disagree",
        )),
        (left, right) => Ok(left.or(right).unwrap_or_default()),
    }
}

/// The template's effort and history switches. An omitted switch, and an
/// effort switch left off, keep the template's default.
pub(super) fn controls(
    template: Option<&TemplateOptions>,
) -> (libmir::ReasoningEffort, libmir::ReasoningHistory) {
    let effort = match template.and_then(|options| options.medium_effort) {
        Some(true) => libmir::ReasoningEffort::Efficient,
        Some(false) | None => libmir::ReasoningEffort::ModelDefault,
    };
    let history = match template.and_then(|options| options.truncate_history_thinking) {
        Some(true) => libmir::ReasoningHistory::Truncate,
        Some(false) => libmir::ReasoningHistory::Keep,
        None => libmir::ReasoningHistory::ModelDefault,
    };
    (effort, history)
}

/// Allowance within, never in addition to, the total completion limit.
/// Explicit schema-tool reasoning reserves a quarter for the tool by default;
/// an explicit allowance still wins. Other decoding modes retain their
/// defaults.
pub(super) fn budget(
    value: Option<u64>,
    mode: ReasoningMode,
    constraints: libmir::ToolConstraints,
    total: Option<u64>,
    image: bool,
) -> Result<Option<std::num::NonZeroUsize>, ApiError> {
    let value = value.or_else(|| {
        (!image && mode == ReasoningMode::Enabled && constraints == libmir::ToolConstraints::Schema)
            .then_some(total)
            .flatten()
            .filter(|total| *total >= 3)
            .map(|total| total - (total / 4).max(2))
    });
    let Some(value) = value else {
        return Ok(None);
    };
    if image || mode != ReasoningMode::Enabled || constraints != libmir::ToolConstraints::Schema {
        return Err(ApiError::bad_request(
            "thinking_token_budget requires enabled reasoning and schema-constrained text tools",
        ));
    }
    if value.checked_add(2).zip(total).is_none_or(|(minimum, total)| minimum > total) {
        return Err(ApiError::bad_request(
            "thinking_token_budget requires explicit max_tokens with room for the delimiter and tool output",
        ));
    }
    let value = usize::try_from(value).map_err(|error| ApiError::bad_request(error.to_string()))?;
    std::num::NonZeroUsize::new(value)
        .map(Some)
        .ok_or_else(|| ApiError::bad_request("thinking_token_budget must be positive"))
}
