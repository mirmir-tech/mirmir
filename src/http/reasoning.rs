use libmir::ReasoningMode;
use serde::Deserialize;

use super::error::ApiError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateOptions {
    pub enable_thinking: Option<bool>,
}

/// Accept the vLLM thinking switch without silently changing a caller's
/// declared mode. Unsupported template options fail during deserialization.
pub(super) fn resolve(
    explicit: Option<ReasoningMode>,
    template: Option<TemplateOptions>,
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
