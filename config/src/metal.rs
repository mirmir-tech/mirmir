use std::path::PathBuf;

use clap::Args;
use libmir::{FeatureToggle, FusionMode, MetalConfig};

#[derive(Debug, Args, Default)]
pub struct MetalArgs {
    #[arg(long, env = "MIRMIR_METAL_DEVICE_TOKEN_PIPELINE", value_parser = parse_bool)]
    device_token_pipeline: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_PREFIX_CACHE_ENTRIES")]
    prefix_cache_entries: Option<usize>,
    #[arg(long, env = "MIRMIR_METAL_PREFIX_CACHE_BYTES")]
    prefix_cache_bytes: Option<usize>,
    #[arg(long, env = "MIRMIR_METAL_PREFILL_STEP")]
    prefill_step: Option<usize>,
    #[arg(long, env = "MIRMIR_METAL_KV_RESERVE_TOKENS")]
    kv_reserve_tokens: Option<usize>,
    #[arg(long, env = "MIRMIR_METAL_PAGED_ATTENTION_MIN_CONTEXT")]
    paged_attention_min_context: Option<usize>,
    #[arg(long, env = "MIRMIR_METAL_NATIVE_PAGED_SDPA", value_parser = parse_bool)]
    force_native_paged_attention: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_PROFILE_LAYERS", value_parser = parse_bool)]
    profile_layers: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_PROFILE_COMPONENTS", value_parser = parse_bool)]
    profile_components: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_PROFILE_GRAPH_BUILD", value_parser = parse_bool)]
    profile_graph_build: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_PREFILL_EVAL_LAYERS")]
    prefill_evaluation_layers: Option<usize>,
    #[arg(long, env = "MIRMIR_METAL_GRAPH_DUMP")]
    graph_dump: Option<PathBuf>,
    #[arg(long, env = "MIRMIR_METAL_FUSED_ATTENTION", value_parser = parse_bool)]
    fused_attention: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_FUSED_DENSE_GATE_UP", value_parser = parse_bool)]
    fused_dense_gate_up: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_FUSED_EXPERT_GATE_UP", value_parser = parse_fusion)]
    routed_expert_gate_up: Option<FusionMode>,
    #[arg(long, env = "MIRMIR_METAL_FUSED_SHARED_EXPERT_GATE_UP", value_parser = parse_fusion)]
    shared_expert_gate_up: Option<FusionMode>,
    #[arg(long, env = "MIRMIR_METAL_FUSED_SHARED_DENSE_GATE_UP", value_parser = parse_bool)]
    shared_dense_gate_up: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_NATIVE_ROUTER", value_parser = parse_bool)]
    native_router: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_COMPILED_GATED_DELTA_DECODE", value_parser = parse_bool)]
    compiled_gated_delta_decode: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_FUSED_GATED_DELTA_DECODE", value_parser = parse_bool)]
    fused_gated_delta_decode: Option<bool>,
    #[arg(long, env = "MIRMIR_METAL_FUSED_GATED_DELTA_NORMALIZATION", value_parser = parse_bool)]
    fused_gated_delta_normalization: Option<bool>,
}

impl MetalArgs {
    pub(super) fn apply_to(&self, config: &mut MetalConfig) {
        assign_toggle(&mut config.fusion.device_token_pipeline, self.device_token_pipeline);
        assign(&mut config.cache.prefix_cache_entries, self.prefix_cache_entries);
        if self.prefix_cache_bytes.is_some() {
            config.cache.prefix_cache_bytes = self.prefix_cache_bytes;
        }
        if self.prefill_step.is_some() {
            config.cache.prefill_step = self.prefill_step;
        }
        assign(&mut config.cache.kv_reserve_tokens, self.kv_reserve_tokens);
        assign(&mut config.cache.paged_attention_min_context, self.paged_attention_min_context);
        assign(
            &mut config.cache.force_native_paged_attention,
            self.force_native_paged_attention,
        );
        assign(&mut config.diagnostics.profile_layers, self.profile_layers);
        assign(&mut config.diagnostics.profile_components, self.profile_components);
        assign(&mut config.diagnostics.profile_graph_build, self.profile_graph_build);
        if self.prefill_evaluation_layers.is_some() {
            config.diagnostics.prefill_evaluation_layers = self.prefill_evaluation_layers;
        }
        if let Some(path) = &self.graph_dump {
            config.diagnostics.graph_dump = Some(path.clone());
        }
        if let Some(enabled) = self.fused_attention {
            config.fusion.hybrid_attention = enabled.into();
            config.fusion.dense_attention = enabled.into();
        }
        if let Some(enabled) = self.fused_dense_gate_up {
            config.fusion.hybrid_dense_gate_up = enabled.into();
            config.fusion.dense_gate_up = enabled.into();
        }
        assign(&mut config.fusion.routed_expert_gate_up, self.routed_expert_gate_up);
        assign(&mut config.fusion.shared_expert_gate_up, self.shared_expert_gate_up);
        assign_toggle(&mut config.fusion.shared_dense_gate_up, self.shared_dense_gate_up);
        assign_toggle(&mut config.fusion.native_router, self.native_router);
        assign_toggle(
            &mut config.fusion.compiled_gated_delta_decode,
            self.compiled_gated_delta_decode,
        );
        assign_toggle(&mut config.fusion.fused_gated_delta_decode, self.fused_gated_delta_decode);
        assign_toggle(
            &mut config.fusion.fused_gated_delta_normalization,
            self.fused_gated_delta_normalization,
        );
    }
}

fn assign<T>(target: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *target = value;
    }
}

fn assign_toggle(target: &mut FeatureToggle, value: Option<bool>) {
    if let Some(value) = value {
        *target = value.into();
    }
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err("expected one of: 1, 0, true, false, yes, no, on, off".into()),
    }
}

fn parse_fusion(value: &str) -> Result<FusionMode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(FusionMode::Auto),
        "1" | "true" | "yes" | "on" | "enabled" => Ok(FusionMode::Enabled),
        "0" | "false" | "no" | "off" | "disabled" => Ok(FusionMode::Disabled),
        _ => Err("expected auto, enabled, or disabled".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_and_named_boolean_values() {
        assert_eq!(parse_bool("1"), Ok(true));
        assert_eq!(parse_bool("off"), Ok(false));
        assert!(parse_bool("sometimes").is_err());
    }

    #[test]
    fn parses_explicit_fusion_policy() {
        assert_eq!(parse_fusion("auto"), Ok(FusionMode::Auto));
        assert_eq!(parse_fusion("enabled"), Ok(FusionMode::Enabled));
        assert_eq!(parse_fusion("0"), Ok(FusionMode::Disabled));
    }
}
