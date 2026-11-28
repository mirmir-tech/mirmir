use std::{fs, path::Path};

use libmir::{
    AdmissionStatus, GenerationOverrides, ModelDescriptor, TemplateKind, WeightContainer,
};
use serde_json::Value;

use crate::{
    config::{GenerationConfig, resolve_source},
    model::{
        class,
        format::{Ecosystem, ModelEncoding, ModelFormat},
    },
};

pub(super) struct ModelPresentation {
    pub class: String,
    pub ecosystem: Ecosystem,
    pub container: Option<WeightContainer>,
    pub encoding: ModelEncoding,
    pub metal_compatibility: AdmissionStatus,
    pub cuda_compatibility: AdmissionStatus,
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

pub(super) fn inspect(
    path: &Path,
    vision: Option<&Path>,
    generation: GenerationConfig,
) -> ModelPresentation {
    let overrides = GenerationOverrides {
        max_tokens: generation.max_tokens,
        min_tokens: None,
        ignore_eos: None,
        temperature: generation.temperature,
        top_p: generation.top_p,
        top_k: generation.top_k,
        repetition_penalty: generation.repetition_penalty,
    };
    let inspected = resolve_source(path, vision).and_then(|source| {
        ModelDescriptor::inspect_source(source.as_model_source(), overrides).map_err(Into::into)
    });
    match inspected {
        Ok(descriptor) => present(path, &descriptor),
        Err(error) => {
            let config = json(path.join("config.json"));
            let template = template_from_dir(path);
            unavailable(&config, template.as_deref(), path, vision, error.to_string())
        },
    }
}

fn present(path: &Path, descriptor: &ModelDescriptor) -> ModelPresentation {
    let config = descriptor.layout().read_config().unwrap_or(Value::Null);
    let template = template_from_layout(descriptor);
    let format = ModelFormat::local(path, &config, Some(descriptor));
    let kind = descriptor.template().kind();
    ModelPresentation {
        class: model_class(&config),
        ecosystem: format.ecosystem,
        container: format.container,
        encoding: format.encoding,
        metal_compatibility: format.metal,
        cuda_compatibility: format.cuda,
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
}

fn unavailable(
    config: &Value,
    template: Option<&str>,
    path: &Path,
    vision: Option<&Path>,
    error: String,
) -> ModelPresentation {
    let format = ModelFormat::local(path, config, None);
    ModelPresentation {
        class: model_class(config),
        ecosystem: format.ecosystem,
        container: format.container,
        encoding: format.encoding,
        metal_compatibility: format.metal,
        cuda_compatibility: format.cuda,
        size_bytes: weight_size(path, vision),
        features: Features {
            tool_use: has_tools(template),
            thinking: has_thinking(template),
            vision: vision.is_some()
                || config.get("vision_config").is_some_and(|value| !value.is_null()),
        },
        loadable: false,
        error,
    }
}

fn json(path: impl AsRef<Path>) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|body| libmir::config_json::parse(&body).ok())
        .unwrap_or(Value::Null)
}

fn template_from_layout(descriptor: &ModelDescriptor) -> Option<String> {
    if let Some(path) = &descriptor.layout().chat_template_path
        && let Ok(text) = fs::read_to_string(path)
    {
        return Some(text);
    }
    let config = descriptor.layout().read_tokenizer_config().ok().flatten()?;
    extract_chat_template(&config)
}

fn template_from_dir(path: &Path) -> Option<String> {
    fs::read_to_string(path.join("chat_template.jinja"))
        .ok()
        .or_else(|| extract_chat_template(&json(path.join("tokenizer_config.json"))))
}

fn extract_chat_template(config: &Value) -> Option<String> {
    let value = config.get("chat_template")?;
    value.as_str().map(str::to_owned).or_else(|| {
        value
            .as_array()?
            .iter()
            .find_map(|entry| entry.get("template").and_then(Value::as_str).map(str::to_owned))
    })
}

/// The configuration's `architectures`, else its `model_type`: a GGUF
/// checkpoint's derived configuration names only the latter.
fn model_class(config: &Value) -> String {
    let architectures = config.get("architectures").and_then(Value::as_array);
    class::name(
        architectures.into_iter().flatten().filter_map(Value::as_str),
        [config.get("model_type").and_then(Value::as_str)],
    )
}

fn weight_size(path: &Path, vision: Option<&Path>) -> u64 {
    if path.is_file() {
        let shards = libmir::gguf::shard_paths(path).unwrap_or_else(|_| vec![path.to_path_buf()]);
        let text_bytes: u64 = shards
            .iter()
            .filter_map(|shard| fs::metadata(shard).ok().map(|metadata| metadata.len()))
            .sum();
        let vision_bytes = vision
            .and_then(|vision| fs::metadata(vision).ok().map(|metadata| metadata.len()))
            .unwrap_or(0);
        return text_bytes.saturating_add(vision_bytes);
    }
    fs::read_dir(path).map_or(0, |entries| {
        entries
            .filter_map(Result::ok)
            .filter(|entry| WeightContainer::of(&entry.path()).is_some())
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::model_class;

    #[test]
    fn names_a_gguf_import_by_its_model_type() {
        assert_eq!(
            model_class(
                &json!({"architectures": ["Gemma4ForConditionalGeneration"], "model_type": "gemma4"})
            ),
            "Gemma4ForConditionalGeneration"
        );
        assert_eq!(model_class(&json!({"model_type": "gemma4"})), "gemma4");
        assert_eq!(model_class(&json!({"architectures": [], "model_type": ""})), "unknown");
    }
}
