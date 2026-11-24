use super::*;

fn files(names: &[(&str, u64)]) -> Vec<RepoFile> {
    names
        .iter()
        .map(|&(path, size)| RepoFile { path: path.into(), size: Some(size) })
        .collect()
}

#[test]
fn groups_quantizations_with_their_shards_and_the_vision_tower() {
    let repository = GgufRepository::from_files(&files(&[
        ("README.md", 1),
        ("config.json", 1),
        ("gemma-4-E4B-it-Q4_K_M.gguf", 50),
        ("gemma-4-E4B-it-UD-Q4_K_XL.gguf", 55),
        ("BF16/gemma-4-E4B-it-BF16-00002-of-00002.gguf", 70),
        ("BF16/gemma-4-E4B-it-BF16-00001-of-00002.gguf", 80),
        ("mmproj-F32.gguf", 9),
        ("mmproj-BF16.gguf", 5),
        ("mmproj-F16.gguf", 5),
        ("MTP/mtp-gemma-4-E4B-it-Q8_0.gguf", 3),
        ("imatrix_unsloth.gguf_file", 2),
    ]));
    let names = repository
        .variants
        .iter()
        .map(|variant| variant.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["Q4_K_M", "UD-Q4_K_XL", "BF16"]);
    let bf16 = repository
        .variant("bf16")
        .map(|variant| (variant.bytes(), variant.shards.len()));
    assert_eq!(bf16, Some((Some(150), 2)));
    assert_eq!(
        repository.variant("BF16").map(|variant| variant.shards[0].path.as_str()),
        Some("BF16/gemma-4-E4B-it-BF16-00001-of-00002.gguf")
    );
    assert!(repository.variant("gemma-4-E4B-it-Q4_K_M").is_some());
    assert!(repository.variant("Q8_0").is_none());
    assert_eq!(repository.vision.map(|file| file.path), Some("mmproj-F16.gguf".into()));
}

#[test]
fn falls_back_to_file_stems_when_labels_collide_or_are_missing() {
    let repository =
        GgufRepository::from_files(&files(&[("a/model-Q4_0.gguf", 1), ("b/model-Q4_0.gguf", 2)]));
    let names = repository
        .variants
        .iter()
        .map(|variant| variant.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["a_model-Q4_0", "b_model-Q4_0"]);
    // Without a type anywhere, the label is the whole stem.
    let plain = GgufRepository::from_files(&files(&[("tiny.gguf", 1)]));
    assert_eq!(plain.variants[0].name, "tiny");
    assert!(
        GgufRepository::from_files(&files(&[("model.safetensors", 1)]))
            .variants
            .is_empty()
    );
}

#[test]
fn finds_a_vision_tower_named_anywhere_and_orders_without_sizes() {
    // Hub search lists files without sizes, and Google names its tower last.
    let names =
        ["gemma-4-E4B-it-mmproj.gguf", "gemma-4-E4B_q4_0-it.gguf", "m-Q8_0.gguf", "m-IQ2_M.gguf"];
    let listed = names.map(|path| RepoFile { path: path.into(), size: None });
    let repository = GgufRepository::from_files(&listed);
    let variants = repository
        .variants
        .iter()
        .map(|variant| variant.name.as_str())
        .collect::<Vec<_>>();
    // A type glued by `_` is the label, ordered by its bits.
    assert_eq!(variants, ["IQ2_M", "q4_0", "Q8_0"]);
    assert_eq!(repository.vision.map(|file| file.path), Some(names[0].into()));
    assert_eq!(repository.variants[0].bytes(), None);
    // 8 billion weights at 8.51 bits.
    assert_eq!(repository.variants[2].estimated_bytes(8_000_000_000), Some(8_510_000_000));
}

#[test]
fn defaults_to_q4_k_m_then_the_largest_that_fits_then_the_smallest() {
    let repository = GgufRepository::from_files(&files(&[
        ("m-Q2_K.gguf", 2),
        ("m-Q4_K_M.gguf", 4),
        ("m-Q8_0.gguf", 8),
    ]));
    let name = |repository: &GgufRepository, budget: u64| {
        repository
            .default_variant(|variant| variant.bytes().is_some_and(|bytes| bytes <= budget))
            .map(|variant| variant.name.clone())
    };
    assert_eq!(name(&repository, 1).as_deref(), Some("Q4_K_M"));
    let without = GgufRepository::from_files(&files(&[("m-Q2_K.gguf", 2), ("m-Q8_0.gguf", 8)]));
    assert_eq!(name(&without, 9).as_deref(), Some("Q8_0"));
    assert_eq!(name(&without, 5).as_deref(), Some("Q2_K"));
    assert_eq!(name(&without, 1).as_deref(), Some("Q2_K"));
    assert_eq!(name(&GgufRepository::default(), 9), None);
}

#[test]
fn reads_bits_per_weight_from_labels() {
    for (label, bits) in [
        ("Q4_K_M", Some(4.9)),
        ("UD-Q4_K_XL", Some(4.9)),
        ("Q4_K_S", Some(4.67)),
        ("Q5_K_S", Some(5.57)),
        ("Q5_K_M", Some(5.7)),
        ("iq2_xxs", Some(2.06)),
        ("IQ2_XS", Some(2.31)),
        ("bf16", Some(16.0)),
        ("F32", Some(32.0)),
        ("MXFP4_MOE", Some(4.25)),
        ("tiny", None),
    ] {
        assert_eq!(quantization::bits_per_weight(label), bits, "{label}");
    }
}
