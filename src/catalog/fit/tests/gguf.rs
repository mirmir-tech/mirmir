use super::*;
use crate::catalog::hub::{GgufRepository, RepoFile};

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
/// from the parameter count with the vision tower, with `Q8_0`, which fits, as
/// the default it is sized by.
#[test]
fn lists_a_gguf_repositorys_variants_in_one_row() {
    let memory = MachineMemory { budget: Some(64 * GIB), ..NO_MEMORY };
    let result = evaluate(gguf("gemma4"), memory);
    let names = result.variants.iter().map(|variant| variant.name.as_str()).collect::<Vec<_>>();
    assert_eq!(names, ["Q2_K", "Q4_K_M", "Q8_0"]);
    assert_eq!(result.default_variant.as_deref(), Some("Q8_0"));
    // 8 billion weights at 4.9 and 8.51 bits, with 600 million at 16 bits
    // in the vision tower.
    assert_eq!(result.variants[1].size, VariantSize::Estimated(6_100_000_000));
    assert_eq!(result.weight_bytes, Some(9_710_000_000));
    assert_eq!(result.memory_fit, MemoryFit::Fits);
    assert_eq!(result.compatibility, AdmissionStatus::Unknown);
    assert_eq!(evaluate(gguf("bert"), memory).compatibility, AdmissionStatus::Unsupported);
}

/// Search estimates the vision tower with each variant as pull counts its
/// listed file: where `Q8_0` fits only without it, both take `Q4_K_M`.
#[test]
fn search_and_pull_choose_the_same_default_with_the_vision_tower() {
    let memory = MachineMemory {
        budget: Some(14_000_000_000),
        ..NO_MEMORY
    };
    let searched = evaluate(gguf("gemma4"), memory);
    assert_eq!(searched.default_variant.as_deref(), Some("Q4_K_M"));
    // The listing pull sees: the estimates' sizes, exact.
    let listed = [
        ("m-Q2_K.gguf", 3_170_000_000),
        ("m-Q4_K_M.gguf", 4_900_000_000),
        ("m-Q8_0.gguf", 8_510_000_000),
        ("mmproj-m-F16.gguf", 1_200_000_000),
    ]
    .map(|(path, size)| RepoFile { path: path.into(), size: Some(size) });
    let (pulled, default) = variants(&GgufRepository::from_files(&listed), None, memory);
    assert_eq!(default, searched.default_variant);
    assert_eq!(pulled[2].size, VariantSize::Exact(9_710_000_000));
    // Without the tower `Q8_0` would fit.
    let (_, text_only) = variants(&GgufRepository::from_files(&listed[..3]), None, memory);
    assert_eq!(text_only.as_deref(), Some("Q8_0"));
}

/// A repository with `SafeTensors` weights beside its GGUF files is pulled
/// as one `SafeTensors` model, so search treats its container, encoding and
/// admission as `SafeTensors` rather than GGUF.
#[test]
fn lists_no_variants_beside_safetensors_weights() {
    let mut mixed = gguf("bert");
    mixed.siblings.push(Sibling {
        rfilename: "model.safetensors".to_owned(),
    });
    mixed.safetensors = model("BF16", 1_000_000_000).safetensors;
    let result = evaluate(mixed, MachineMemory { budget: Some(64 * GIB), ..NO_MEMORY });
    assert!(result.variants.is_empty());
    assert_eq!(result.default_variant, None);
    assert_eq!(result.container, Some(WeightContainer::SafeTensors));
    assert_eq!(result.encoding, ModelEncoding::Unknown);
    assert_eq!(result.compatibility, AdmissionStatus::Unknown);
    assert_eq!(result.weight_bytes, Some(2_000_000_000));

    // Even when the Hub omits the `safetensors` summary object, a listed
    // `.safetensors` sibling still marks the repository for SafeTensors
    // header inspection, matching pull.
    let mut mixed_without_summary = gguf("gemma4");
    mixed_without_summary.siblings.push(Sibling {
        rfilename: "model.safetensors".to_owned(),
    });
    let result = evaluate(mixed_without_summary, NO_MEMORY);
    assert_eq!(result.container, Some(WeightContainer::SafeTensors));
    assert_eq!(result.compatibility, AdmissionStatus::Unknown);
}

/// A row is downloaded when its own pull target is: a GGUF-only
/// repository's default variant, not any variant, else the repository.
#[test]
fn a_row_is_downloaded_by_its_own_pull_target() -> crate::error::Result<()> {
    let mut row = evaluate(gguf("gemma4"), MachineMemory { budget: Some(64 * GIB), ..NO_MEMORY });
    row.variants[0].downloaded = true;
    assert!(!row.target_downloaded(|| Ok(true))?);
    row.variants[2].downloaded = true;
    assert!(row.target_downloaded(|| Ok(false))?);
    let plain = evaluate(model("BF16", 1), NO_MEMORY);
    assert!(plain.target_downloaded(|| Ok(true))?);
    assert!(!plain.target_downloaded(|| Ok(false))?);
    Ok(())
}
