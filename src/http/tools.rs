use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RequestTool {
    #[serde(rename = "type", default = "function_kind")]
    kind: String,
    function: RequestFunctionDefinition,
}

#[derive(Debug, Deserialize)]
struct RequestFunctionDefinition {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct RequestToolCall {
    id: String,
    #[serde(rename = "type", default = "function_kind")]
    kind: String,
    function: RequestFunctionCall,
}

#[derive(Debug, Deserialize)]
struct RequestFunctionCall {
    name: String,
    #[serde(deserialize_with = "json_or_encoded_json")]
    arguments: serde_json::Value,
}

impl From<RequestTool> for libmir::Tool {
    fn from(tool: RequestTool) -> Self {
        Self {
            kind: tool.kind,
            function: libmir::FunctionDefinition {
                name: tool.function.name,
                description: tool.function.description,
                parameters: tool.function.parameters,
            },
        }
    }
}

impl From<RequestToolCall> for libmir::ToolCall {
    fn from(call: RequestToolCall) -> Self {
        Self {
            id: call.id,
            kind: call.kind,
            function: libmir::FunctionCall {
                name: call.function.name,
                arguments: call.function.arguments,
            },
        }
    }
}

fn json_or_encoded_json<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    if let serde_json::Value::String(encoded) = &value {
        Ok(serde_json::from_str(encoded).unwrap_or(value))
    } else {
        Ok(value)
    }
}

fn function_kind() -> String {
    "function".into()
}
