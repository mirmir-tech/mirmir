use libmir::ToolChoice;

pub fn tool_choice(value: Option<serde_json::Value>) -> Result<ToolChoice, String> {
    let Some(value) = value else {
        return Ok(ToolChoice::Auto);
    };
    if let Some(choice) = value.as_str() {
        return match choice {
            "auto" => Ok(ToolChoice::Auto),
            "none" => Ok(ToolChoice::None),
            "required" => Ok(ToolChoice::Required),
            value => Err(format!("unsupported tool_choice `{value}`")),
        };
    }
    let function = value
        .as_object()
        .filter(|choice| choice.get("type").and_then(serde_json::Value::as_str) == Some("function"))
        .and_then(|choice| choice.get("function"))
        .and_then(serde_json::Value::as_object)
        .and_then(|function| function.get("name"))
        .and_then(serde_json::Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            "tool_choice must be `auto`, `none`, `required`, or a named function".to_owned()
        })?;
    Ok(ToolChoice::Function(function.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_function_choice() {
        let choice = tool_choice(Some(serde_json::json!({
            "type": "function",
            "function": {"name": "weather"}
        })))
        .expect("valid named function");
        assert_eq!(choice, ToolChoice::Function("weather".into()));
    }

    #[test]
    fn parses_standard_choice_modes() {
        assert_eq!(tool_choice(None), Ok(ToolChoice::Auto));
        assert_eq!(tool_choice(Some(serde_json::json!("none"))), Ok(ToolChoice::None));
        assert_eq!(tool_choice(Some(serde_json::json!("required"))), Ok(ToolChoice::Required));
        assert!(tool_choice(Some(serde_json::json!("unsupported"))).is_err());
    }
}
