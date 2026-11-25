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
fn defaults_to_q8_0_that_fits_then_q4_k_m_then_the_largest_that_fits_then_the_smallest() {
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
    // Q8_0 when it fits, else Q4_K_M even when that does not fit either.
    assert_eq!(name(&repository, 8).as_deref(), Some("Q8_0"));
    assert_eq!(name(&repository, 7).as_deref(), Some("Q4_K_M"));
    assert_eq!(name(&repository, 1).as_deref(), Some("Q4_K_M"));
    // Without Q4_K_M: Q8_0 when it fits, even beside a larger one that fits.
    let without = GgufRepository::from_files(&files(&[
        ("m-Q2_K.gguf", 2),
        ("m-Q8_0.gguf", 8),
        ("m-BF16.gguf", 16),
    ]));
    assert_eq!(name(&without, 20).as_deref(), Some("Q8_0"));
    // Then the largest that fits, else the smallest.
    assert_eq!(name(&without, 5).as_deref(), Some("Q2_K"));
    assert_eq!(name(&without, 1).as_deref(), Some("Q2_K"));
    let unquantized =
        GgufRepository::from_files(&files(&[("m-Q2_K.gguf", 2), ("m-BF16.gguf", 16)]));
    assert_eq!(name(&unquantized, 20).as_deref(), Some("BF16"));
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

/// Shard suffixes are read as bytes: accented and non-Latin stems stay whole,
/// a malformed suffix is part of the stem, and a numbered shard loses it.
#[test]
fn unshards_without_cutting_characters() {
    assert_eq!(unsharded("model-aaaaé12345678.gguf"), "model-aaaaé12345678");
    assert_eq!(unsharded("dir/modèle-Q4_K_M-00002-of-00003.gguf"), "dir/modèle-Q4_K_M");
    assert_eq!(unsharded("模型-量化-00001-OF-00002.gguf"), "模型-量化");
    assert_eq!(unsharded("Qwen3-8B-Q4_K_M.gguf"), "Qwen3-8B-Q4_K_M");
    assert_eq!(unsharded("m-0001-of-00002.gguf"), "m-0001-of-00002");
    assert_eq!(unsharded("m-00003-of-00002.gguf"), "m-00003-of-00002");
    assert_eq!(unsharded("m-00001-of-0000x.gguf"), "m-00001-of-0000x");
}

/// The largest type that fits is a known one: a type of unknown size,
/// ordered last, is taken only when no known one fits.
#[test]
fn prefers_a_known_type_as_the_largest_that_fits() {
    let repository = GgufRepository::from_files(&files(&[
        ("m-Q2_K.gguf", 2),
        ("m-Q5_K_M.gguf", 5),
        ("m-custom.gguf", 3),
    ]));
    let name = |budget: u64| {
        repository
            .default_variant(|variant| variant.bytes().is_some_and(|bytes| bytes <= budget))
            .map(|variant| variant.name.clone())
    };
    assert_eq!(name(9).as_deref(), Some("Q5_K_M"));
    assert_eq!(name(4).as_deref(), Some("Q2_K"));
    let unknown = GgufRepository::from_files(&files(&[("m-custom.gguf", 3), ("n-other.gguf", 4)]));
    assert_eq!(
        unknown
            .default_variant(|variant| variant.bytes().is_some_and(|bytes| bytes <= 9))
            .map(|variant| variant.name.as_str()),
        Some("n-other")
    );
}

/// Variants named after their file stems keep their quantization: its bits,
/// its estimate and the `Q8_0` default.
#[test]
fn keeps_the_quantization_of_variants_named_by_stem() {
    let listed = ["a/m-Q8_0.gguf", "b/m-Q8_0.gguf", "a/m-Q4_K_M.gguf"]
        .map(|path| RepoFile { path: path.into(), size: None });
    let repository = GgufRepository::from_files(&listed);
    let names = repository
        .variants
        .iter()
        .map(|variant| (variant.name.as_str(), variant.quantization.label()))
        .collect::<Vec<_>>();
    assert_eq!(names, [("a_m-Q4_K_M", "Q4_K_M"), ("a_m-Q8_0", "Q8_0"), ("b_m-Q8_0", "Q8_0")]);
    assert_eq!(repository.variants[1].estimated_bytes(8_000_000_000), Some(8_510_000_000));
    let default = repository.default_variant(|_| true).map(|variant| variant.name.as_str());
    assert_eq!(default, Some("a_m-Q8_0"));
    let fallback = repository.default_variant(|_| false).map(|variant| variant.name.as_str());
    assert_eq!(fallback, Some("a_m-Q4_K_M"));
}

/// Only a listing without `SafeTensors` weights is GGUF only, and a vision
/// tower is estimated at the precision its name gives.
#[test]
fn tells_gguf_only_listings_and_estimates_the_vision_tower() {
    let gguf = files(&[("m-Q4_K_M.gguf", 4), ("mmproj-F32.gguf", 1)]);
    let repository = GgufRepository::from_files(&gguf);
    assert!(repository.gguf_only());
    // 600 million weights at 32 bits.
    assert_eq!(repository.estimated_vision_bytes(), Some(2_400_000_000));
    let mixed = files(&[("m-Q4_K_M.gguf", 4), ("model.safetensors", 8)]);
    assert!(!GgufRepository::from_files(&mixed).gguf_only());
    assert!(!GgufRepository::from_files(&files(&[("model.safetensors", 8)])).gguf_only());
    let unnamed = GgufRepository::from_files(&files(&[("m-Q8_0.gguf", 8), ("mmproj.gguf", 1)]));
    assert_eq!(unnamed.estimated_vision_bytes(), Some(1_200_000_000));
    assert_eq!(GgufRepository::default().estimated_vision_bytes(), None);
}
