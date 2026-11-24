use std::collections::BTreeMap;

use libmir::EncodingKind;

use super::{variants::VariantSize, *};
use crate::catalog::hub::{HubConfig, HubGguf, SafeTensors, Sibling};

/// An uninspected remote model, for the fields a test sets.
impl Default for CatalogModel {
    fn default() -> Self {
        Self {
            id: String::new(),
            downloads: 0,
            likes: 0,
            gated: false,
            model_class: String::new(),
            compatibility: AdmissionStatus::Unknown,
            memory_fit: MemoryFit::Unknown,
            weight_bytes: None,
            required_bytes: None,
            budget_bytes: None,
            confidence: Confidence::Low,
            reason: String::new(),
            downloaded: false,
            local_source: "remote",
            ecosystem: Ecosystem::Unknown,
            container: None,
            encoding: ModelEncoding::Unknown,
            metal_compatibility: AdmissionStatus::Unknown,
            cuda_compatibility: AdmissionStatus::Unknown,
            preflight_bytes: None,
            preflight_error: None,
            features: Features::default(),
            variants: Vec::new(),
            default_variant: None,
        }
    }
}

const NO_MEMORY: MachineMemory = MachineMemory {
    total: None,
    available: None,
    budget: None,
    source: "test",
};

fn model(dtype: &str, parameters: u64) -> HubModel {
    HubModel {
        id: "Qwen/Test".to_owned(),
        library_name: Some("transformers".to_owned()),
        downloads: 10,
        likes: 2,
        gated: serde_json::Value::Bool(false),
        config: HubConfig {
            architectures: vec!["Qwen2ForCausalLM".to_owned()],
        },
        safetensors: Some(SafeTensors {
            parameters: BTreeMap::from([(dtype.to_owned(), parameters)]),
        }),
        tags: vec!["safetensors".to_owned()],
        gguf: None,
        siblings: Vec::new(),
    }
}

#[test]
fn estimates_bf16_weights_and_fits_with_headroom() {
    let memory = MachineMemory {
        total: Some(16 * GIB),
        available: Some(12 * GIB),
        budget: Some(10 * GIB),
        source: "test",
    };
    let result = evaluate(model("BF16", 1_000_000_000), memory);
    assert_eq!(result.compatibility, AdmissionStatus::Unknown);
    assert_eq!(result.weight_bytes, Some(2_000_000_000));
    assert_eq!(result.memory_fit, MemoryFit::Fits);
}

#[test]
fn refuses_to_guess_unknown_dtype() {
    let memory = MachineMemory {
        total: Some(16 * GIB),
        available: Some(12 * GIB),
        budget: Some(10 * GIB),
        source: "test",
    };
    let result = evaluate(model("CUSTOM", 1_000_000_000), memory);
    assert_eq!(result.weight_bytes, None);
    assert_eq!(result.memory_fit, MemoryFit::Unknown);
    assert_eq!(result.confidence, Confidence::Low);
}

#[test]
fn exposes_qwen3_search_capabilities_before_download() {
    let mut candidate = model("BF16", 1_000_000_000);
    candidate.id = "Qwen/Qwen3-8B".to_owned();
    let result = evaluate(candidate, NO_MEMORY);

    assert_eq!(result.ecosystem, Ecosystem::Transformers);
    assert_eq!(result.container, Some(WeightContainer::SafeTensors));
    assert_eq!(result.encoding, ModelEncoding::Unknown);
    assert_eq!(result.metal_compatibility, AdmissionStatus::Unknown);
    assert_eq!(result.cuda_compatibility, AdmissionStatus::Unknown);
    assert!(result.features.tool_use);
    assert!(result.features.thinking);
    assert!(!result.features.vision);
}

#[test]
fn ranks_supported_fitting_models_first() {
    let memory = MachineMemory {
        total: Some(8 * GIB),
        available: Some(6 * GIB),
        budget: Some(5 * GIB),
        source: "test",
    };
    let fitting = evaluate(model("BF16", 500_000_000), memory);
    let oversized = evaluate(model("BF16", 8_000_000_000), memory);
    assert_eq!(compare(&fitting, &oversized), Ordering::Less);
}

#[test]
fn ranks_admission_before_memory_fit() {
    let ranked = |compatibility, memory_fit| CatalogModel {
        compatibility,
        memory_fit,
        ..CatalogModel::default()
    };
    let order = [
        ranked(AdmissionStatus::Supported, MemoryFit::Fits),
        ranked(AdmissionStatus::Supported, MemoryFit::Tight),
        ranked(AdmissionStatus::Supported, MemoryFit::Unknown),
        ranked(AdmissionStatus::Supported, MemoryFit::DoesNotFit),
        ranked(AdmissionStatus::Partial, MemoryFit::Fits),
        ranked(AdmissionStatus::Unknown, MemoryFit::Fits),
        ranked(AdmissionStatus::Unsupported, MemoryFit::Fits),
    ];
    for pair in order.windows(2) {
        assert_eq!(compare(&pair[0], &pair[1]), Ordering::Less, "{pair:?}");
    }
}

/// A GGUF repository has no `SafeTensors` metadata; its tag, in any case,
/// still makes it a GGUF model awaiting admission. (One without GGUF files
/// offers nothing to download and is unsupported.)
#[test]
fn remote_gguf_awaits_admission_like_safetensors() {
    let mut gguf = model("BF16", 1_000_000_000);
    gguf.safetensors = None;
    gguf.library_name = None;
    gguf.tags = vec!["GGUF".to_owned(), "Conversational".to_owned()];
    gguf.siblings = vec![Sibling {
        rfilename: "model-Q4_K_M.gguf".to_owned(),
    }];
    let facts = gguf.facts();
    assert_eq!(facts.container, Some(WeightContainer::Gguf));
    assert_eq!(facts.encoding, Some(EncodingKind::Ggml));
    let format = ModelFormat::remote(None, facts);
    assert_eq!(format.ecosystem, Ecosystem::Ggml);
    assert_eq!(
        (format.metal, format.cuda),
        (AdmissionStatus::Unknown, AdmissionStatus::Unknown)
    );
    let result = evaluate(gguf, NO_MEMORY);
    assert_eq!(result.compatibility, AdmissionStatus::Unknown);
    assert_eq!(result.container, Some(WeightContainer::Gguf));
    assert_eq!(result.encoding, ModelEncoding::Family(EncodingKind::Ggml));
    assert_eq!(result.metal_compatibility, AdmissionStatus::Unknown);
    assert_eq!(result.cuda_compatibility, AdmissionStatus::Unknown);
}

fn gguf(architecture: &str) -> HubModel {
    let files = ["README.md", "m-Q8_0.gguf", "m-Q4_K_M.gguf", "m-Q2_K.gguf", "mmproj-m-F16.gguf"];
    HubModel {
        id: "owner/m-GGUF".to_owned(),
        library_name: Some("gguf".to_owned()),
        safetensors: None,
        tags: vec!["gguf".to_owned()],
        gguf: Some(HubGguf {
            total: Some(8_000_000_000),
            architecture: Some(architecture.to_owned()),
        }),
        siblings: files
            .iter()
            .map(|&rfilename| Sibling { rfilename: rfilename.to_owned() })
            .collect(),
        ..model("BF16", 1)
    }
}

/// A GGUF repository is one row whose variants come from its files, sized
/// from the parameter count, with `Q4_K_M` as the default it is sized by.
#[test]
fn lists_a_gguf_repositorys_variants_in_one_row() {
    let memory = MachineMemory { budget: Some(64 * GIB), ..NO_MEMORY };
    let result = evaluate(gguf("gemma4"), memory);
    let names = result.variants.iter().map(|variant| variant.name.as_str()).collect::<Vec<_>>();
    assert_eq!(names, ["Q2_K", "Q4_K_M", "Q8_0"]);
    assert_eq!(result.default_variant.as_deref(), Some("Q4_K_M"));
    // 8 billion weights at 4.9 bits.
    assert_eq!(result.variants[1].size, VariantSize::Estimated(4_900_000_000));
    assert_eq!(result.weight_bytes, Some(4_900_000_000));
    assert_eq!(result.memory_fit, MemoryFit::Fits);
    assert_eq!(result.compatibility, AdmissionStatus::Unknown);
    assert_eq!(evaluate(gguf("bert"), memory).compatibility, AdmissionStatus::Unsupported);
}
