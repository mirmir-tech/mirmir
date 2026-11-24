use super::*;

fn files(names: &[(&str, u64)]) -> Vec<RepoFile> {
    names.iter().map(|&(path, size)| RepoFile { path: path.into(), size }).collect()
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
    assert_eq!(bf16, Some((150, 2)));
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
    let plain = GgufRepository::from_files(&files(&[("tiny.gguf", 1)]));
    assert_eq!(plain.variants[0].name, "tiny");
    assert!(
        GgufRepository::from_files(&files(&[("model.safetensors", 1)]))
            .variants
            .is_empty()
    );
}
