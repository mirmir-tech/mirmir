use super::{GgufRepository, files};

/// Every published variant name resolves to its own shards, even when another
/// variant sorts earlier and shares that name as its file stem basename.
#[test]
fn resolves_every_published_variant_name_to_its_own_shards() {
    let repository =
        GgufRepository::from_files(&files(&[("demo-Q4_0.gguf", 1), ("alt/demo-Q4_0.gguf", 2)]));
    for variant in &repository.variants {
        let resolved = repository
            .variant(&variant.name)
            .expect("every published variant resolves by its own name");
        assert_eq!(resolved.shards, variant.shards, "{}", variant.name);
        let uppercase = variant.name.to_ascii_uppercase();
        let resolved_upper =
            repository.variant(&uppercase).expect("canonical lookup is case-insensitive");
        assert_eq!(resolved_upper.shards, variant.shards, "{uppercase}");
    }
    assert_eq!(
        repository.variant("demo-Q4_0").map(|variant| variant.shards[0].path.as_str()),
        Some("demo-Q4_0.gguf")
    );
    assert_eq!(
        repository
            .variant("alt_demo-Q4_0")
            .map(|variant| variant.shards[0].path.as_str()),
        Some("alt/demo-Q4_0.gguf")
    );
}

/// Reordering an unchanged file listing never swaps which file each variant ID
/// refers to, whether names collide through path sanitization or letter case.
#[test]
fn assigns_the_same_variant_ids_regardless_of_listing_order() {
    let pairs = |listing: &[(&str, u64)]| {
        GgufRepository::from_files(&files(listing))
            .variants
            .into_iter()
            .map(|variant| (variant.name, variant.shards[0].path.clone()))
            .collect::<Vec<_>>()
    };

    let sanitized_a = pairs(&[("a/b-Q4_0.gguf", 1), ("a_b-Q4_0.gguf", 2)]);
    let sanitized_b = pairs(&[("a_b-Q4_0.gguf", 2), ("a/b-Q4_0.gguf", 1)]);
    assert_eq!(sanitized_a, sanitized_b);
    assert_eq!(
        sanitized_a,
        vec![
            ("a_b-Q4_0".to_owned(), "a_b-Q4_0.gguf".to_owned()),
            ("a_b-Q4_0_2".to_owned(), "a/b-Q4_0.gguf".to_owned()),
        ]
    );

    let cased_a = pairs(&[("foo-Q4_0.gguf", 1), ("Foo-Q4_0.gguf", 2)]);
    let cased_b = pairs(&[("Foo-Q4_0.gguf", 2), ("foo-Q4_0.gguf", 1)]);
    assert_eq!(cased_a, cased_b);
}
