use libmir::{
    models::{generation::GenerationSettings, layout::DecoderConfig, tokenizer::TextTokenizer},
    runtime::{
        backend::{BackendInfo, SamplingLogits},
        sampling::{Sampler, SamplerConfig},
    },
};

pub(super) fn sampler_config(
    settings: &GenerationSettings,
    seed: Option<u64>,
    vocab_size: usize,
) -> SamplerConfig {
    SamplerConfig {
        temperature: settings.temperature,
        top_p: settings.top_p,
        top_k: settings.top_k,
        repetition_penalty: settings.repetition_penalty,
        vocab_size: Some(vocab_size),
        seed: seed.unwrap_or_else(|| SamplerConfig::default().seed),
    }
}

pub(super) fn sampling_vocab_limit(
    text_tokenizer: &TextTokenizer,
    decoder: &DecoderConfig,
) -> usize {
    text_tokenizer.vocab_size().min(decoder.vocab_size)
}

pub(super) fn sampling_logits(settings: &GenerationSettings, vocab_size: usize) -> SamplingLogits {
    if greedy(settings) {
        return SamplingLogits::None;
    }
    if settings.repetition_penalty <= 1.0 && settings.top_k > 0 && settings.top_k < vocab_size {
        if settings.top_p < 1.0 {
            return SamplingLogits::Sample {
                vocab_size,
                temperature: settings.temperature,
                top_p: settings.top_p,
                top_k: settings.top_k,
                draw: 0.0,
            };
        }
        return SamplingLogits::SampleTopK {
            k: settings.top_k,
            vocab_size,
            temperature: settings.temperature,
            draw: 0.0,
        };
    }
    SamplingLogits::Full
}

pub(super) fn request_sampling_logits(
    settings: &GenerationSettings,
    vocab_size: usize,
    sampler: &mut Sampler,
) -> SamplingLogits {
    let mut sampling = sampling_logits(settings, vocab_size);
    match &mut sampling {
        SamplingLogits::SampleTopK { draw, .. } | SamplingLogits::Sample { draw, .. } => {
            *draw = sampler.draw_unit_f32();
        },
        SamplingLogits::None | SamplingLogits::Full | SamplingLogits::TopK { .. } => {},
    }
    sampling
}

pub(super) fn sampling_backend_line(
    settings: &GenerationSettings,
    vocab_size: usize,
    backend: &BackendInfo,
) -> String {
    let accelerator = accelerator_name(backend);
    if greedy(settings) {
        return format!("sampling_backend: native {accelerator} argmax after output head");
    }
    if settings.top_p < 1.0 {
        return if matches!(sampling_logits(settings, vocab_size), SamplingLogits::Sample { .. }) {
            format!("sampling_backend: native {accelerator} top-p/top-k sampler")
        } else {
            "sampling_backend: full logits to Rust sampler for top-p".into()
        };
    }
    if matches!(sampling_logits(settings, vocab_size), SamplingLogits::SampleTopK { .. }) {
        return format!("sampling_backend: native {accelerator} top-k sampler");
    }
    "sampling_backend: full logits to Rust sampler".into()
}

fn accelerator_name(backend: &BackendInfo) -> &'static str {
    let name = backend.name.to_ascii_lowercase();
    if name.contains("cuda") {
        "CUDA"
    } else if name.contains("metal") || name.contains("mlx") {
        "Metal"
    } else {
        "accelerator"
    }
}

fn greedy(settings: &GenerationSettings) -> bool {
    (settings.temperature <= f32::EPSILON || settings.top_k == 1)
        && settings.repetition_penalty <= 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_full_logits_when_top_k_is_disabled() {
        let settings = GenerationSettings {
            max_tokens: 1,
            temperature: 0.8,
            top_p: 1.0,
            top_k: 0,
            repetition_penalty: 1.0,
        };

        assert_eq!(sampling_logits(&settings, 32), SamplingLogits::Full);
    }

    #[test]
    fn keeps_top_p_sampling_on_accelerator_when_top_k_is_bounded() {
        let settings = GenerationSettings {
            max_tokens: 1,
            temperature: 1.0,
            top_p: 0.95,
            top_k: 20,
            repetition_penalty: 1.0,
        };

        assert!(matches!(sampling_logits(&settings, 248_320), SamplingLogits::Sample { .. }));
    }
}
