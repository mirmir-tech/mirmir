use std::{fs, path::Path};

use libmir::{GenerationOverrides, ModelDescriptor, models::chat::TemplateKind};
use serde_json::Value;

use crate::config::GenerationConfig;

pub(super) struct ModelPresentation {
    pub class: String,
    pub library: String,
    pub size_bytes: u64,
    pub features: Features,
    pub loadable: bool,
    pub error: String,
}

pub(super) struct Features {
    pub tool_use: bool,
    pub thinking: bool,
    pub vision: bool,
}

pub(super) fn inspect(path: &Path, generation: GenerationConfig) -> ModelPresentation {
    let config = json(path.join("config.json"));
    let template = template(path);
    let overrides = GenerationOverrides {
        max_tokens: generation.max_tokens,
        temperature: generation.temperature,
        top_p: generation.top_p,
        top_k: generation.top_k,
        repetition_penalty: generation.repetition_penalty,
    };
    match ModelDescriptor::inspect(path, overrides) {
        Ok(descriptor) => {
            let kind = descriptor.template().kind();
            ModelPresentation {
                class: model_class(&config),
                library: library(path, &config),
                size_bytes: descriptor.layout().weights.iter().map(|weight| weight.bytes).sum(),
                features: Features {
                    tool_use: has_tools(template.as_deref()),
                    thinking: matches!(kind, TemplateKind::QwenChatMl | TemplateKind::Gemma4)
                        || has_thinking(template.as_deref()),
                    vision: descriptor.vision().is_some(),
                },
                loadable: true,
                error: String::new(),
            }
        },
        Err(error) => ModelPresentation {
            class: model_class(&config),
            library: library(path, &config),
            size_bytes: weight_size(path),
            features: Features {
                tool_use: has_tools(template.as_deref()),
                thinking: has_thinking(template.as_deref()),
                vision: config.get("vision_config").is_some_and(|value| !value.is_null()),
            },
            loadable: false,
            error: error.to_string(),
        },
    }
}

fn json(path: impl AsRef<Path>) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|body| serde_json::from_str(&body).ok())
        .unwrap_or(Value::Null)
}

fn template(path: &Path) -> Option<String> {
    fs::read_to_string(path.join("chat_template.jinja")).ok().or_else(|| {
        let config = json(path.join("tokenizer_config.json"));
        let value = config.get("chat_template")?;
        value.as_str().map(str::to_owned).or_else(|| {
            value
                .as_array()?
                .iter()
                .find_map(|entry| entry.get("template").and_then(Value::as_str).map(str::to_owned))
        })
    })
}

fn model_class(config: &Value) -> String {
    config
        .get("architectures")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn library(path: &Path, config: &Value) -> String {
    if fs::read_dir(path).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "gguf"))
    }) {
        return "GGUF".to_owned();
    }
    if config.get("quantization").is_some() {
        "MLX".to_owned()
    } else {
        "SafeTensors".to_owned()
    }
}

fn weight_size(path: &Path) -> u64 {
    fs::read_dir(path).map_or(0, |entries| {
        entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "safetensors" || ext == "gguf")
            })
            .filter_map(|entry| entry.metadata().ok().map(|metadata| metadata.len()))
            .sum()
    })
}

fn has_tools(template: Option<&str>) -> bool {
    template.is_some_and(|value| value.contains("tool_calls") || value.contains("tools"))
}

fn has_thinking(template: Option<&str>) -> bool {
    template.is_some_and(|value| {
        value.contains("enable_thinking")
            || value.contains("<think>")
            || value.contains("<|think|>")
    })
}
