//! A legacy variant's move where a case-insensitive file system opens its
//! file by a namesake's spelling, and where moves run side by side.

use std::fs;

use super::legacy;
use crate::error::Result;

/// Saving the repository `org/repo--q4_k_m`, whose key is the legacy key
/// of the variant `Org/Repo:Q4_K_M` in another case, moves the variant
/// first: on a case-insensitive file system both name one file, here
/// renamed to the namesake's spelling as such a file system finds it.
#[test]
fn a_namesake_in_another_case_moves_the_legacy_variant_first() -> Result<()> {
    let old = "Org--Repo--Q4_K_M";
    let (store, paths, root) = legacy("legacy-namesake-case", "Org/Repo:Q4_K_M", old)?;
    let spelled = paths.models_dir.join("org--repo--q4_k_m.toml");
    fs::rename(paths.models_dir.join(format!("{old}.toml")), &spelled)?;
    let snapshot = root.join("cache/snapshots/abc");
    fs::create_dir_all(&snapshot)?;
    store.save_hub_model("org/repo--q4_k_m", "main", "abc", snapshot.clone())?;
    let variant = store.resolve_model("Org/Repo:Q4_K_M")?;
    assert_eq!(variant.key, "Org--Repo@Q4_K_M");
    assert_eq!(variant.path, paths.gguf_variants_dir.join("Org--Repo@Q4_K_M").canonicalize()?);
    assert_eq!(variant.generation.max_tokens, Some(8));
    // Resolved paths are canonical (on macOS the temporary directory is
    // reached through /var -> /private/var).
    assert_eq!(store.resolve_model("org/repo--q4_k_m")?.path, snapshot.canonicalize()?);
    assert_eq!(store.list_model_configs()?.len(), 2);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// Legacy variants found by several listings at once move once each, none
/// failing on a step another made first. The in-process lock takes them
/// one at a time; the steps would hold without it, as they must against
/// another process's move.
#[test]
fn concurrent_listings_move_each_legacy_variant_once() -> Result<()> {
    let (store, paths, root) = legacy("legacy-concurrent", "owner/m:Q8_0", "owner--m--Q8_0")?;
    for variant in ["Q4_0", "Q5_K_M", "Q6_K"] {
        let view = paths.gguf_variants_dir.join(format!("owner--m--{variant}"));
        fs::create_dir_all(&view)?;
        let marker = paths.gguf_variants_dir.join(format!("owner--m--{variant}.pending"));
        fs::write(marker, format!("owner/m:{variant}"))?;
        let config = fs::read_to_string(paths.models_dir.join("owner--m--Q8_0.toml"))?
            .replace("Q8_0", variant);
        fs::write(paths.models_dir.join(format!("owner--m--{variant}.toml")), config)?;
    }
    std::thread::scope(|scope| {
        let listings = (0..8).map(|_| scope.spawn(|| store.list_model_configs()));
        // Every listing runs before any is joined.
        let listings = listings.collect::<Vec<_>>();
        for listing in listings {
            let listed = listing
                .join()
                .map_err(|_| crate::error::Error::Config("a listing panicked".to_owned()));
            listed??;
        }
        Ok::<_, crate::error::Error>(())
    })?;
    let mut keys = store
        .list_model_configs()?
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(keys, ["owner--m@Q4_0", "owner--m@Q5_K_M", "owner--m@Q6_K", "owner--m@Q8_0"]);
    for key in &keys {
        let reference = key.replacen("--", "/", 1).replacen('@', ":", 1);
        assert!(store.hub_model_downloaded(&reference)?, "{reference}");
        assert!(paths.gguf_variants_dir.join(key).is_dir(), "{key}");
    }
    for variant in ["Q4_0", "Q5_K_M", "Q6_K"] {
        let marker = paths.gguf_variants_dir.join(format!("owner--m@{variant}.pending"));
        assert!(marker.exists(), "{variant}");
    }
    fs::remove_dir_all(root)?;
    Ok(())
}
