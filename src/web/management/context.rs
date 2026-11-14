//! A model's context in the management API's JSON.

use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::application::{ContextChoice, ModelContext};

/// `"automatic"`, or `{"tokens": 32768}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextJson {
    Automatic,
    Tokens(NonZeroU64),
}

#[derive(Serialize)]
pub(super) struct ContextInfo {
    block_tokens: u64,
    min_tokens: u64,
    max_tokens: u64,
    saved: ContextJson,
    automatic_tokens: u64,
    resident_tokens: Option<u64>,
}

impl From<ContextJson> for ContextChoice {
    fn from(value: ContextJson) -> Self {
        match value {
            ContextJson::Automatic => Self::Automatic,
            ContextJson::Tokens(tokens) => Self::Tokens(tokens),
        }
    }
}

impl From<ContextChoice> for ContextJson {
    fn from(value: ContextChoice) -> Self {
        match value {
            ContextChoice::Automatic => Self::Automatic,
            ContextChoice::Tokens(tokens) => Self::Tokens(tokens),
        }
    }
}

impl From<ModelContext> for ContextInfo {
    fn from(value: ModelContext) -> Self {
        Self {
            block_tokens: value.limits.block_tokens.get(),
            min_tokens: value.limits.min_tokens(),
            max_tokens: value.limits.max_tokens,
            saved: value.saved.into(),
            automatic_tokens: value.automatic_tokens,
            resident_tokens: value.resident_tokens,
        }
    }
}

/// The `context` query parameter: `automatic` or a token count.
pub(super) fn query(value: Option<&str>) -> Result<Option<ContextChoice>, String> {
    match value {
        None => Ok(None),
        Some("automatic") => Ok(Some(ContextChoice::Automatic)),
        Some(text) => text
            .parse::<NonZeroU64>()
            .map(|tokens| Some(ContextChoice::Tokens(tokens)))
            .map_err(|_| format!("context `{text}` is not `automatic` or a token count")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_and_query_name_the_same_choices() -> Result<(), String> {
        let tokens: ContextJson =
            serde_json::from_str(r#"{"tokens":32768}"#).map_err(|error| error.to_string())?;
        assert_eq!(ContextChoice::from(tokens), query(Some("32768"))?.ok_or("missing")?);
        let automatic: ContextJson =
            serde_json::from_str(r#""automatic""#).map_err(|error| error.to_string())?;
        assert_eq!(ContextChoice::from(automatic), ContextChoice::Automatic);
        assert!(query(Some("0")).is_err());
        assert!(query(Some("auto")).is_err());
        Ok(())
    }
}
