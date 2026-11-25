use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static NEXT_STORE: AtomicU64 = AtomicU64::new(0);

#[test]
fn creates_stable_key_and_rejects_traversal() -> Result<()> {
    assert_eq!(model_key("Qwen/Qwen2.5")?, "Qwen--Qwen2.5");
    assert!(model_key("../secret").is_err());
    assert!(model_key("owner/model/extra").is_err());
    Ok(())
}

#[test]
fn persists_and_removes_hub_model_record() -> Result<()> {
    let unique = NEXT_STORE.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("mirmir-hub-model-{}-{unique}", std::process::id()));
    let store = Store::new(super::super::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    ));
    let snapshot = root.join("cache/snapshots/0123456789");
    fs::create_dir_all(&snapshot)?;
    let model = store.save_hub_model("Qwen/Test", "main", "0123456789", snapshot.clone())?;
    assert_eq!(model.hub.as_ref().map(|hub| hub.commit.as_str()), Some("0123456789"));
    assert!(store.hub_model_downloaded("Qwen/Test")?);
    assert_eq!(store.resolve_model("Qwen/Test")?.key, "Qwen--Test");
    assert_eq!(store.resolve_model(snapshot.to_str().unwrap())?.key, "Qwen--Test");
    assert!(store.remove_hub_model_config("Qwen/Test")?);
    assert!(!store.hub_model_downloaded("Qwen/Test")?);
    Ok(())
}

#[test]
fn model_runtime_section_is_optional_and_reaches_load_options() -> Result<()> {
    let unique = NEXT_STORE.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("mirmir-model-runtime-{}-{unique}", std::process::id()));
    let store = Store::new(super::super::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    ));
    let snapshot = root.join("cache/snapshots/0123456789");
    fs::create_dir_all(&snapshot)?;
    store.save_hub_model("Qwen/Reranker", "main", "0123456789", snapshot)?;
    let path = store.model_config_path("Qwen--Reranker");
    let written = fs::read_to_string(&path)?;
    assert!(!written.contains("[runtime]"));
    assert_eq!(store.resolve_model("Qwen/Reranker")?.runtime.kv_blocks, None);
    fs::write(&path, format!("{written}\n[runtime]\nkv_blocks = 512\n"))?;
    let options = store.resolve_model("Qwen/Reranker")?.runtime.load_options(true);
    assert_eq!(options.kv_blocks.map(std::num::NonZeroU32::get), Some(512));
    assert!(options.allow_memory_overcommit);
    // Pulling a new snapshot keeps the saved context.
    let next = root.join("cache/snapshots/abcdef0123");
    fs::create_dir_all(&next)?;
    store.save_hub_model("Qwen/Reranker", "main", "abcdef0123", next)?;
    let kept = store.resolve_model("Qwen/Reranker")?.runtime.kv_blocks;
    assert_eq!(kept.map(std::num::NonZeroU32::get), Some(512));
    Ok(())
}

/// A GGUF variant that does not import fails to resolve with the import's
/// error, not with a missing `config.json` later; its selector still names
/// it.
#[test]
fn a_gguf_import_failure_surfaces_from_resolve() -> Result<()> {
    let unique = NEXT_STORE.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("mirmir-gguf-resolve-{}-{unique}", std::process::id()));
    let store = Store::new(super::super::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    ));
    let view = root.join("variant");
    fs::create_dir_all(&view)?;
    fs::write(view.join("m-Q4_0.gguf"), b"not a GGUF header")?;
    store.save_hub_model("owner/model-GGUF:Q4_0", "main", "abc", view)?;
    let error = store.resolve_model("owner/model-GGUF:Q4_0").map(|model| model.path);
    assert!(
        error.as_ref().is_err_and(|error| error.to_string().contains("GGUF")),
        "{error:?}"
    );
    assert_eq!(store.selected_model("owner/model-GGUF:Q4_0")?.key, "owner--model-GGUF@Q4_0");
    fs::remove_dir_all(root)?;
    Ok(())
}

/// `model:Q8_0`, a variant of an ownerless repository, and `model/Q8_0`, a
/// repository, keep their own configurations; each resolves by name, and a
/// repository keeps the key it always had.
#[test]
fn an_ownerless_variant_and_a_namesake_repository_stay_apart() -> Result<()> {
    let unique = NEXT_STORE.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("mirmir-ownerless-{}-{unique}", std::process::id()));
    let store = Store::new(super::super::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    ));
    let (variant, repository) = (root.join("variant"), root.join("repository"));
    for path in [&variant, &repository] {
        fs::create_dir_all(path)?;
    }
    store.save_hub_model("model:Q8_0", "main", "abc", variant.clone())?;
    store.save_hub_model("model/Q8_0", "main", "def", repository.clone())?;
    assert_eq!(model_key("model/Q8_0")?, "model--Q8_0");
    for (selector, max_tokens) in [("model:Q8_0", 8), ("model/Q8_0", 16)] {
        let generation = GenerationConfig {
            max_tokens: Some(max_tokens),
            ..Default::default()
        };
        let change = super::super::ModelConfigChange {
            generation: Some(generation),
            runtime: None,
        };
        store.save_model_config(selector, "", None, change)?;
    }
    let selected = |selector| store.selected_model(selector);
    let (first, second) = (selected("model:Q8_0")?, selected("model/Q8_0")?);
    assert_eq!((first.path, first.generation.max_tokens), (variant, Some(8)));
    assert_eq!((second.path, second.generation.max_tokens), (repository, Some(16)));
    let saved = store.list_model_configs()?;
    assert_eq!(saved.len(), 2);
    assert!(store.hub_model_downloaded("model:Q8_0")? && store.hub_model_downloaded("model/Q8_0")?);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A variant saved under its legacy key, `owner--name--VARIANT`, moves to
/// `owner--name@VARIANT` with its view and pending marker, and resolves and
/// removes as before.
#[test]
fn a_variant_saved_under_its_legacy_key_moves_to_the_current_one() -> Result<()> {
    let unique = NEXT_STORE.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("mirmir-legacy-variant-{}-{unique}", std::process::id()));
    let paths =
        super::super::Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let store = Store::new(paths.clone());
    let legacy = "owner--model-GGUF--Q4_K_M";
    let view = paths.gguf_variants_dir.join(legacy);
    fs::create_dir_all(&view)?;
    // Its import, keyed by the legacy view's path, holds a derived file.
    let import = store.gguf_import_dir(&view.canonicalize()?);
    fs::create_dir_all(&import)?;
    fs::write(import.join("tokenizer.json"), "{}")?;
    fs::write(
        paths.gguf_variants_dir.join(format!("{legacy}.pending")),
        "owner/model-GGUF:Q4_K_M",
    )?;
    fs::create_dir_all(&paths.models_dir)?;
    fs::write(
        paths.models_dir.join(format!("{legacy}.toml")),
        format!(
            "schema_version = 1\nid = \"{legacy}\"\npath = \"{}\"\n\n[hub]\nrepo_id = \
             \"owner/model-GGUF:Q4_K_M\"\nrevision = \"main\"\ncommit = \"abc\"\n",
            view.canonicalize()?.display()
        ),
    )?;
    let resolved = store.resolve_model("owner/model-GGUF:Q4_K_M")?;
    let key = "owner--model-GGUF@Q4_K_M";
    assert_eq!(resolved.key, key);
    assert_eq!(resolved.path, paths.gguf_variants_dir.join(key).canonicalize()?);
    assert!(!view.exists() && !paths.models_dir.join(format!("{legacy}.toml")).exists());
    let moved = store.gguf_import_dir(&resolved.path);
    assert!(!import.exists() && moved.join("tokenizer.json").exists(), "{moved:?}");
    assert!(paths.gguf_variants_dir.join(format!("{key}.pending")).exists());
    assert_eq!(store.list_model_configs()?.len(), 1);
    assert!(store.remove_hub_model_config("owner/model-GGUF:Q4_K_M")?);
    fs::remove_dir_all(root)?;
    Ok(())
}
