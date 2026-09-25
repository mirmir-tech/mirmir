use super::ChatRequest;
use crate::http::{error::ApiError, media::messages};

impl ChatRequest {
    pub fn into_application(
        self,
    ) -> Result<(String, libmir::GenerationRequest, Option<Vec<u8>>), ApiError> {
        if self.model.trim().is_empty() {
            return Err(ApiError::bad_request("model cannot be empty"));
        }
        if self.messages.is_empty() {
            return Err(ApiError::bad_request("messages cannot be empty"));
        }
        if self.n.unwrap_or(1) != 1 {
            return Err(ApiError::bad_request("only n=1 is supported"));
        }
        if self.reasoning_budget.is_some()
            || self.reasoning_effort.is_some()
            || self.thinking_token_budget.is_some()
        {
            return Err(ApiError::bad_request(
                "reasoning_budget, thinking_token_budget and reasoning_effort are unsupported; use reasoning with a total max_completion_tokens budget",
            ));
        }
        // These OpenAI controls are additive penalties, not repetition_penalty.
        // Do not silently ignore them or substitute different sampling
        // semantics.
        for (name, value) in [
            ("presence_penalty", self.presence_penalty),
            ("frequency_penalty", self.frequency_penalty),
        ] {
            if value.is_some_and(|value| value != 0.0) {
                return Err(ApiError::bad_request(format!(
                    "{name} is unsupported; omit it or set it to zero"
                )));
            }
        }
        let max_tokens = match (self.max_tokens, self.max_completion_tokens) {
            (Some(left), Some(right)) if left != right => {
                return Err(ApiError::bad_request("max_tokens and max_completion_tokens disagree"));
            },
            (left, right) => left.or(right),
        };
        let (messages, image) = messages(self.messages)?;
        let selector = self.model;
        let request = libmir::GenerationRequest {
            conversation: libmir::Conversation {
                messages,
                tools: self.tools.into_iter().map(Into::into).collect(),
                tool_choice: crate::protocol::openai::tool_choice(self.tool_choice)
                    .map_err(ApiError::bad_request)?,
            },
            options: libmir::GenerationOverrides {
                max_tokens: optional_usize(max_tokens)?,
                min_tokens: optional_usize(self.min_tokens)?,
                ignore_eos: self.ignore_eos,
                temperature: self.temperature,
                top_p: self.top_p,
                top_k: optional_usize(self.top_k)?,
                repetition_penalty: self.repetition_penalty,
            },
            seed: self.seed,
            tool_constraints: self.tool_constraints,
            reasoning_cycle: libmir::ReasoningCyclePolicy::default(),
            reasoning: crate::http::reasoning::resolve(self.reasoning, self.chat_template_kwargs)?,
        };
        Ok((selector, request, image))
    }
}

fn optional_usize(value: Option<u64>) -> Result<Option<usize>, ApiError> {
    value
        .map(usize::try_from)
        .transpose()
        .map_err(|error| ApiError::bad_request(error.to_string()))
}
