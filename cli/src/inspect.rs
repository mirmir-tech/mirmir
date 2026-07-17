use std::path::Path;

use libmir::models::{
    execution::ExecutionPlan,
    layout::{AttentionLayerType, DecoderConfig, ModelLayout, ModelMetadata},
    weights::{DecoderTensorSchema, TensorCatalog},
};

use crate::{error::CliError, write_lines};

pub fn run(path: &Path) -> Result<(), CliError> {
    let layout = ModelLayout::inspect(path)?;
    let metadata = ModelMetadata::from_layout(&layout)?;
    let decoder = DecoderConfig::from_layout(&layout)?;
    let tensors = TensorCatalog::from_layout(&layout)?;
    let schema = DecoderTensorSchema::discover(&decoder, &tensors);
    let readiness = schema.readiness(&tensors);
    let support = native_support(&metadata, &decoder, &tensors);
    let (linear_attention_layers, full_attention_layers) = attention_layer_counts(&decoder);
    let weight_bytes: u64 = layout.weights.iter().map(|weight| weight.bytes).sum();
    tracing::info!(
        root = %layout.root.display(),
        family = ?metadata.family,
        model_type = metadata.model_type.as_deref().unwrap_or("unknown"),
        context_len = metadata.context_len,
        tensors = tensors.len(),
        weight_files = layout.weights.len(),
        weight_bytes,
        readiness = %readiness.summary(),
        execution = %support.summary(),
        "model inspected"
    );
    for missing in &readiness.missing {
        tracing::warn!(root = %layout.root.display(), missing = %missing, "model tensor missing");
    }
    let mut lines = vec![
        format!("root: {}", layout.root.display()),
        format!("family: {:?}", metadata.family),
        format!("model_type: {}", metadata.model_type.as_deref().unwrap_or("unknown")),
        format!("architectures: {}", metadata.architectures.join(", ")),
        format!("dtype: {}", metadata.dtype.as_deref().unwrap_or("unknown")),
        format!("context_len: {}", metadata.context_len),
        format!("quantization: {:?}", metadata.quantization),
        format!("quantization_group_size: {}", optional_usize(metadata.quantization_group_size)),
        format!(
            "quantization_mode: {}",
            metadata.quantization_mode.as_deref().unwrap_or("unknown")
        ),
        format!("decoder_layers: {}", decoder.num_hidden_layers),
        format!("decoder_hidden: {}", decoder.hidden_size),
        format!("decoder_heads: {}/{}", decoder.num_attention_heads, decoder.num_key_value_heads),
        format!("decoder_head_dim: {}", decoder.head_dim),
        format!("decoder_tied_output: {}", decoder.tie_word_embeddings),
        format!("decoder_rope_scaling: {:?}", decoder.rope_scaling),
        format!("decoder_partial_rotary_factor: {}", optional_f64(decoder.partial_rotary_factor)),
        format!("decoder_rope_layout: {:?}", decoder.rope_layout),
        format!("decoder_sliding_window: {}", optional_usize(decoder.sliding_window)),
        format!("decoder_linear_attention: {:?}", decoder.linear_attention),
        format!("decoder_linear_attention_layers: {linear_attention_layers}"),
        format!("decoder_full_attention_layers: {full_attention_layers}"),
        format!("decoder_attention_output: {:?}", decoder.attention_output),
        format!("decoder_experts: {}", optional_usize(decoder.num_experts)),
        format!("decoder_top_k_experts: {}", optional_usize(decoder.top_k_experts)),
        format!("decoder_moe_intermediate: {}", optional_usize(decoder.moe_intermediate_size)),
        format!(
            "decoder_shared_expert_intermediate: {}",
            optional_usize(decoder.shared_expert_intermediate_size)
        ),
        format!(
            "decoder_activation: {}",
            decoder.hidden_activation.as_deref().unwrap_or("unknown")
        ),
        format!(
            "decoder_final_logit_softcapping: {}",
            optional_f64(decoder.final_logit_softcapping)
        ),
        format!("tokenizer: {}", layout.has_tokenizer()),
        format!("checkpoint_configuration: {}", layout.configuration_path.is_some()),
        format!("processor_config: {}", layout.processor_config_path.is_some()),
        format!("preprocessor_config: {}", layout.preprocessor_config_path.is_some()),
        format!("video_processor_config: {}", layout.video_processor_config_path.is_some()),
        format!("weight_files: {}", layout.weights.len()),
        format!("weight_bytes: {weight_bytes}"),
        format!("tensors: {}", tensors.len()),
        format!("quantized_weight_tensors: {}", quantized_weight_tensors(&tensors)),
        format!("has_embeddings: {}", has_embeddings(&tensors)),
        format!("has_lm_head: {}", has_lm_head(&tensors)),
        format!("decoder_readiness: {}", readiness.summary()),
        format!("execution_plan: {}", support.plan_summary()),
        format!("native_execution: {}", support.summary()),
    ];
    lines.extend(support.blockers.iter().map(|blocker| format!("execution_blocker: {blocker}")));
    write_lines(lines)
}

struct NativeSupport {
    ready: bool,
    plan: Option<ExecutionPlan>,
    blockers: Vec<String>,
}

impl NativeSupport {
    fn summary(&self) -> String {
        if self.ready {
            format!("native {} decoder path ready", self.plan_summary())
        } else {
            format!("blocked by {}", self.blockers.join("; "))
        }
    }

    fn plan_summary(&self) -> String {
        self.plan.map_or_else(
            || "unsupported".into(),
            |plan| format!("{:?} / {:?} / {:?}", plan.decoder, plan.attention, plan.feed_forward),
        )
    }
}

fn native_support(
    metadata: &ModelMetadata,
    decoder: &DecoderConfig,
    tensors: &TensorCatalog,
) -> NativeSupport {
    let mut blockers = Vec::new();
    let plan = match ExecutionPlan::discover(decoder, tensors) {
        Ok(plan) => Some(plan),
        Err(error) => {
            blockers.push(error.to_string());
            None
        },
    };
    if plan.is_some() && metadata.quantization_group_size.is_none() {
        blockers.push("native quantized weights need quantization group_size".into());
    }
    if plan.is_some_and(|plan| !plan.is_native_implemented()) {
        blockers.push(
            "native execution path for the detected hybrid linear MoE decoder is pending".into(),
        );
    }
    NativeSupport {
        ready: blockers.is_empty(),
        plan,
        blockers,
    }
}

fn has_embeddings(tensors: &TensorCatalog) -> bool {
    tensors.contains("model.embed_tokens.weight")
        || tensors.contains("language_model.model.embed_tokens.weight")
        || tensors.contains("model.language_model.embed_tokens.weight")
        || tensors.contains("transformer.embedding.word_embeddings.weight")
}

fn attention_layer_counts(decoder: &DecoderConfig) -> (usize, usize) {
    decoder.layer_types.iter().fold((0, 0), |(linear, full), layer| match layer {
        AttentionLayerType::Linear => (linear + 1, full),
        AttentionLayerType::Full => (linear, full + 1),
        AttentionLayerType::Sliding => (linear, full),
    })
}

fn has_lm_head(tensors: &TensorCatalog) -> bool {
    tensors.contains("lm_head.weight")
        || tensors.contains("language_model.lm_head.weight")
        || tensors.contains("model.language_model.lm_head.weight")
}

fn quantized_weight_tensors(tensors: &TensorCatalog) -> usize {
    tensors
        .tensors
        .iter()
        .filter(|tensor| {
            matches!(tensor.dtype.as_str(), "U32" | "U8")
                && tensor.name.ends_with(".weight")
                && quant_params_present(tensors, &tensor.name)
        })
        .count()
}

fn quant_params_present(tensors: &TensorCatalog, weight_name: &str) -> bool {
    weight_name.strip_suffix(".weight").is_some_and(|prefix| {
        (tensors.contains(&format!("{prefix}.scales"))
            && tensors.contains(&format!("{prefix}.biases")))
            || (tensors.contains(&format!("{prefix}.weight_scale"))
                && tensors.contains(&format!("{prefix}.weight_scale_2"))
                && tensors.contains(&format!("{prefix}.input_scale")))
    })
}

fn optional_usize(value: Option<usize>) -> String {
    value.map_or_else(|| "none".into(), |number| number.to_string())
}

fn optional_f64(value: Option<f64>) -> String {
    value.map_or_else(|| "none".into(), |number| number.to_string())
}
