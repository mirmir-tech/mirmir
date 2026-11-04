use std::path::PathBuf;

use libmir::decision::{Answer, DecisionBackend, DecisionState, SchemaField, Verdict};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::http::error::ApiError;

/// A Laya checkpoint, a state, and a flat JSON schema of questions.
#[derive(Debug, Deserialize)]
pub(in crate::http) struct DecideRequest {
    pub checkpoint: PathBuf,
    #[serde(default)]
    pub backend: BackendName,
    pub state: StateInput,
    pub schema: Value,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub(in crate::http) enum BackendName {
    #[default]
    Cpu,
    #[cfg(target_os = "macos")]
    Metal,
}

impl BackendName {
    pub const fn backend(self) -> DecisionBackend {
        match self {
            Self::Cpu => DecisionBackend::Cpu,
            #[cfg(target_os = "macos")]
            Self::Metal => DecisionBackend::Metal,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::http) enum StateInput {
    /// Plain text; truncation keeps its beginning.
    Text(String),
    /// Turns oldest first; truncation keeps the newest.
    Conversation(Vec<Value>),
}

impl StateInput {
    pub fn into_state(self) -> Result<DecisionState, ApiError> {
        match self {
            Self::Text(text) => Ok(DecisionState::text(text)),
            Self::Conversation(turns) => DecisionState::conversation(&turns)
                .map_err(|error| ApiError::bad_request(error.to_string())),
        }
    }
}

#[derive(Debug, Serialize)]
pub(in crate::http) struct DecideResponse {
    pub values: Map<String, Value>,
    pub fields: Vec<FieldAnswer>,
}

#[derive(Debug, Serialize)]
pub(in crate::http) struct FieldAnswer {
    pub name: String,
    pub options: Vec<String>,
    pub probabilities: Vec<f64>,
    pub confidence: f64,
    pub concentration: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_level: Option<f64>,
}

impl FieldAnswer {
    pub fn new(field: &SchemaField, answer: Answer) -> Self {
        Self {
            name: field.name.clone(),
            options: field.question.rendered_options(),
            expected_level: match answer.verdict {
                Verdict::Score { expected, .. } => Some(expected),
                Verdict::Choice { .. } | Verdict::YesNo { .. } => None,
            },
            probabilities: answer.probabilities,
            confidence: answer.confidence,
            concentration: answer.concentration,
        }
    }
}
