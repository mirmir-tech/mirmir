use serde::Deserialize;

use super::error::ApiError;

/// The `OpenAI` `response_format` request field.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    Text,
    /// Any JSON object.
    JsonObject,
    JsonSchema {
        json_schema: JsonSchema,
    },
}

#[derive(Debug, Deserialize)]
pub struct JsonSchema {
    pub name: String,
    pub schema: serde_json::Value,
    /// Decoding always follows the schema; `false` is not offered.
    pub strict: Option<bool>,
}

impl ResponseFormat {
    pub(super) fn into_libmir(self) -> Result<libmir::ResponseFormat, ApiError> {
        match self {
            Self::Text => Ok(libmir::ResponseFormat::Text),
            Self::JsonObject => {
                Ok(libmir::ResponseFormat::JsonSchema(serde_json::json!({"type": "object"})))
            },
            Self::JsonSchema { json_schema } => {
                if json_schema.name.trim().is_empty() {
                    return Err(ApiError::bad_request("response_format json_schema needs a name"));
                }
                if json_schema.strict == Some(false) {
                    return Err(ApiError::bad_request(
                        "response_format json_schema is always strict; omit strict or set it true",
                    ));
                }
                Ok(libmir::ResponseFormat::JsonSchema(json_schema.schema))
            },
        }
    }
}
