//! Variants saved under their legacy keys, `owner--name--VARIANT`, before
//! a variant's key joined it to its repository with `@`.

use std::{fs, path::PathBuf};

use super::{NEXT_STORE, Ordering};
use crate::{
    config::{Paths, Store},
    error::Result,
};

/// A store with `reference`'s variant saved as an earlier version did:
/// its configuration, with `max_tokens`, and view under the legacy key.
fn legacy(label: &str, reference: &str, legacy: &str) -> Result<(Store, Paths, PathBuf)> {
    let unique = NEXT_STORE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("mirmir-{label}-{}-{unique}", std::process::id()));
    drop(fs::remove_dir_all(&root));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let view = paths.gguf_variants_dir.join(legacy);
    fs::create_dir_all(&view)?;
    fs::create_dir_all(&paths.models_dir)?;
    fs::write(
        paths.models_dir.join(format!("{legacy}.toml")),
        format!(
            "schema_version = 1\nid = \"{legacy}\"\npath = \"{}\"\n\n[hub]\nrepo_id = \
             \"{reference}\"\nrevision = \"main\"\ncommit = \"abc\"\n\n[generation]\nmax_tokens \
             = 8\n",
            view.canonicalize()?.display()
        ),
    )?;
    Ok((Store::new(paths.clone()), paths, root))
}

/// A variant saved under its legacy key moves to `owner--name@VARIANT`
/// with its view, import and pending marker, and resolves and removes as
/// before.
#[test]
fn a_variant_saved_under_its_legacy_key_moves_to_the_current_one() -> Result<()> {
    let old = "owner--model-GGUF--Q4_K_M";
    let (store, paths, root) = legacy("legacy-variant", "owner/model-GGUF:Q4_K_M", old)?;
    let view = paths.gguf_variants_dir.join(old);
    // Its import, keyed by the legacy view's path, holds a derived file.
    let import = store.gguf_import_dir(&view.canonicalize()?);
    fs::create_dir_all(&import)?;
    fs::write(import.join("tokenizer.json"), "{}")?;
    fs::write(
        paths.gguf_variants_dir.join(format!("{old}.pending")),
        "owner/model-GGUF:Q4_K_M",
    )?;
    let resolved = store.resolve_model("owner/model-GGUF:Q4_K_M")?;
    let key = "owner--model-GGUF@Q4_K_M";
    assert_eq!(resolved.key, key);
    assert_eq!(resolved.path, paths.gguf_variants_dir.join(key).canonicalize()?);
    assert!(!view.exists() && !paths.models_dir.join(format!("{old}.toml")).exists());
    let moved = store.gguf_import_dir(&resolved.path);
    assert!(!import.exists() && moved.join("tokenizer.json").exists(), "{moved:?}");
    assert!(paths.gguf_variants_dir.join(format!("{key}.pending")).exists());
    assert_eq!(store.list_model_configs()?.len(), 1);
    assert!(store.remove_hub_model_config("owner/model-GGUF:Q4_K_M")?);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// Saving the repository `m/Q8` first after an upgrade, whose key is the
/// variant `m:Q8`'s legacy one, moves the variant away: both keep their
/// own settings and paths.
#[test]
fn a_namesake_repository_saved_first_keeps_the_legacy_variant() -> Result<()> {
    let (store, paths, root) = legacy("legacy-namesake", "m:Q8", "m--Q8")?;
    let snapshot = root.join("cache/snapshots/abc");
    fs::create_dir_all(&snapshot)?;
    store.save_hub_model("m/Q8", "main", "abc", snapshot.clone())?;
    let variant = store.selected_model("m:Q8")?;
    assert_eq!((variant.key.as_str(), variant.generation.max_tokens), ("m@Q8", Some(8)));
    assert_eq!(variant.path, paths.gguf_variants_dir.join("m@Q8").canonicalize()?);
    let repository = store.selected_model("m/Q8")?;
    assert_eq!((repository.key.as_str(), repository.generation.max_tokens), ("m--Q8", None));
    assert_eq!(repository.path, snapshot);
    assert!(store.hub_model_downloaded("m:Q8")? && store.hub_model_downloaded("m/Q8")?);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A migration stopped after moving the view and before saving the new
/// configuration resumes from there: the variant loads from its new view.
#[test]
fn an_interrupted_migration_resumes_after_the_view_moved() -> Result<()> {
    let old = "owner--m--Q8";
    let (store, paths, root) = legacy("legacy-interrupted", "owner/m:Q8", old)?;
    let view = paths.gguf_variants_dir.join("owner--m@Q8");
    fs::rename(paths.gguf_variants_dir.join(old), &view)?;
    assert!(store.hub_model_downloaded("owner/m:Q8")?);
    assert_eq!(store.selected_model("owner/m:Q8")?.path, view.canonicalize()?);
    assert!(!paths.models_dir.join(format!("{old}.toml")).exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A selection saved under the legacy key, active before a restart, comes
/// back as the variant's current key once a listing moved it.
#[test]
fn an_active_legacy_selection_follows_its_variant() -> Result<()> {
    let old = "owner--m--Q8";
    let (store, _, root) = legacy("legacy-active", "owner/m:Q8", old)?;
    let state = root.join("config/state.toml");
    fs::write(
        &state,
        format!("schema_version = 1\nrecent_models = []\nactive_models = [\"{old}\"]\n"),
    )?;
    assert_eq!(store.list_model_configs()?.len(), 1);
    let restarted =
        Store::new(Paths::from_roots(root.join("config"), root.join("state"), &root.join("run")));
    assert_eq!(restarted.active_models()?, ["owner--m@Q8"]);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// The legacy variant `m:Q8`, active as `m--Q8`, stays the active model
/// after the repository `m/Q8` is saved under that key and the store opens
/// again.
#[test]
fn an_active_legacy_variant_survives_its_namesake() -> Result<()> {
    let (store, paths, root) = legacy("legacy-active-namesake", "m:Q8", "m--Q8")?;
    fs::write(
        root.join("config/state.toml"),
        "schema_version = 1\nrecent_models = [\"m--Q8\"]\nactive_models = [\"m--Q8\"]\n",
    )?;
    let snapshot = root.join("cache/snapshots/abc");
    fs::create_dir_all(&snapshot)?;
    store.save_hub_model("m/Q8", "main", "abc", snapshot)?;
    let reopened = Store::new(paths.clone());
    assert_eq!(reopened.active_models()?, ["m@Q8"]);
    assert_eq!(reopened.recent_models()?, ["m@Q8"]);
    let active = reopened.resolve_model("m@Q8")?;
    assert_eq!(active.path, paths.gguf_variants_dir.join("m@Q8").canonicalize()?);
    assert_eq!(active.generation.max_tokens, Some(8));
    fs::remove_dir_all(root)?;
    Ok(())
}

/// Another spelling of a variant's key, a second name of its configuration
/// as on a case-insensitive file system, is no legacy key: looking it up
/// moves nothing and removes nothing.
#[test]
fn another_spelling_of_a_variant_key_keeps_its_configuration() -> Result<()> {
    let unique = NEXT_STORE.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("mirmir-spelling-{}-{unique}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let store = Store::new(paths.clone());
    let view = paths.gguf_variants_dir.join("owner--model-GGUF@Q4_0");
    fs::create_dir_all(&view)?;
    store.save_hub_model("owner/model-GGUF:Q4_0", "main", "abc", view.canonicalize()?)?;
    let saved = paths.models_dir.join("owner--model-GGUF@Q4_0.toml");
    let spelling = paths.models_dir.join("owner--model-GGUF@q4_0.toml");
    // A case-insensitive file system (macOS APFS) already names the
    // configuration so; elsewhere a hard link stands in for it.
    let linked = match fs::hard_link(&saved, &spelling) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(error) => return Err(error.into()),
    };
    let lookup = store.hub_model_downloaded("owner/model-GGUF:q4_0");
    // There the spelling finds the installed variant, as anywhere a name of
    // its own does not stand in the way.
    if !linked {
        assert!(lookup?);
    }
    assert!(saved.exists() && spelling.exists());
    // Only the stand-in goes: on a case-insensitive file system the
    // spelling is the configuration itself.
    if linked {
        fs::remove_file(spelling)?;
    }
    assert!(store.hub_model_downloaded("owner/model-GGUF:Q4_0")?);
    assert!(store.hub_model_downloaded("owner/model-GGUF:q4_0")?);
    assert_eq!(store.resolve_model("owner/model-GGUF:q4_0")?.key, "owner--model-GGUF@Q4_0");
    assert!(view.exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A selection of a legacy key spelled in another case finds its variant
/// once it moved.
#[test]
fn a_legacy_selection_in_another_case_finds_its_variant() -> Result<()> {
    let (store, _, root) = legacy("legacy-case", "owner/m:Q8", "owner--m--Q8")?;
    assert_eq!(store.list_model_configs()?.len(), 1);
    assert_eq!(store.selected_model("OWNER--m--q8")?.key, "owner--m@Q8");
    fs::remove_dir_all(root)?;
    Ok(())
}

mod race;
