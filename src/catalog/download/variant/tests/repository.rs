//! A repository's identity across spellings and caches: what a removal of
//! it, or of its last variant, takes.

use std::{fs, path::Path};

use super::{cache, pinned};
use crate::{
    catalog::{Catalog, download::variant::pending},
    config::{HubReference, Paths, Store},
    error::Result,
};

/// Whether `directory` tells names apart by case alone, as Linux does and
/// macOS APFS does not.
fn case_sensitive(directory: &Path) -> Result<bool> {
    fs::create_dir_all(directory)?;
    fs::write(directory.join("probe"), "")?;
    let sensitive = !directory.join("PROBE").exists();
    fs::remove_file(directory.join("probe"))?;
    Ok(sensitive)
}

/// The last variant of a repository, downloading before any cache of its
/// own exists, goes without the repository's cache another tool keeps.
#[tokio::test]
async fn a_last_variant_never_takes_another_tools_cache() -> Result<()> {
    let root = std::env::temp_dir().join(format!("mirmir-external-{}", std::process::id()));
    drop(fs::remove_dir_all(&root));
    let mut paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let external = root.join("hf/hub/models--owner--lone-GGUF/snapshots/abc");
    fs::create_dir_all(&external)?;
    fs::write(external.join("model.gguf"), b"theirs")?;
    paths.external_hub_cache = Some(root.join("hf/hub"));
    let store = Store::new(paths);
    let variant = "owner/lone-GGUF:Q4_0".parse::<HubReference>()?;
    pending::begin(&store, &variant, &pinned(), &[])?;
    let catalog = Catalog::new(store);
    assert!(catalog.remove("owner/lone-GGUF:Q4_0").await?.removed);
    assert_eq!(fs::read(external.join("model.gguf"))?, b"theirs");
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A variant is one model in every spelling: removing it takes its
/// download marker spelled one way and its installation spelled another
/// together, leaving no configuration without its files; its last
/// removal then takes the repository's cache.
#[tokio::test]
async fn every_spelling_of_a_variant_goes_together() -> Result<()> {
    let (store, repo, root) = cache("every-variant-spelling")?;
    let downloading = "Owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    pending::begin(&store, &downloading, &pinned(), &[])?;
    let catalog = Catalog::new(store);
    assert!(catalog.remove("Owner/model-GGUF:Q4_0").await?.removed);
    let store = &catalog.store;
    assert!(crate::catalog::pending_variants(store.paths()).is_empty());
    assert!(!store.hub_model_downloaded("owner/model-GGUF:Q4_0")?);
    for model in store.list_model_configs()? {
        assert!(model.path.exists(), "{} lost its files", model.id);
    }
    assert!(store.hub_model_downloaded("owner/model-GGUF:Q8_0")?);
    assert!(catalog.remove("owner/model-GGUF:q8_0").await?.removed);
    assert!(store.list_model_configs()?.is_empty() && !repo.exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// The last variant's removal leaves the repository's cache while a pull of
/// another of its variants is under way, before that pull wrote anything.
#[tokio::test]
async fn a_last_variant_leaves_the_cache_to_a_pull_under_way() -> Result<()> {
    let (store, repo, root) = cache("pull-under-way")?;
    let catalog = Catalog::new(store);
    let pull = catalog.reserve("owner/model-GGUF:Q6_K", crate::catalog::pull::Activity::Pull)?;
    assert!(catalog.remove("owner/model-GGUF:Q4_0").await?.removed);
    assert!(catalog.remove("owner/model-GGUF:Q8_0").await?.removed);
    assert!(repo.exists());
    drop(pull);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// Removing a repository takes its own configuration in every spelling it
/// was saved under, and the cache of each, where the file system keeps
/// them apart.
#[tokio::test]
async fn a_repository_goes_in_every_spelling() -> Result<()> {
    let (store, repo, root) = cache("every-spelling")?;
    let cache_dir = store.paths().hub_cache_dir.clone();
    if !case_sensitive(&cache_dir)? {
        fs::remove_dir_all(root)?;
        return Ok(());
    }
    let other = cache_dir.join("models--Owner--model-GGUF/snapshots/def");
    fs::create_dir_all(&other)?;
    store.save_hub_model("Owner/model-GGUF", "main", "def", other.clone())?;
    store.save_hub_model("owner/model-GGUF", "main", "abc", repo.join("snapshots/abc"))?;
    let catalog = Catalog::new(store);
    assert!(catalog.remove("owner/model-GGUF").await?.removed);
    assert!(catalog.store.list_model_configs()?.is_empty());
    assert!(!repo.exists() && !other.exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A pull of a repository spelled otherwise than another model of it was
/// saved takes the saved spelling, so its files join that model's cache.
#[test]
fn a_pull_takes_the_saved_spelling() -> Result<()> {
    let (store, _, root) = cache("pull-spelling")?;
    let asked = "OWNER/Model-gguf:Q5_K_M".parse::<HubReference>()?;
    let spelled = crate::catalog::download::spelled(&store, &asked)?;
    assert_eq!(spelled.to_string(), "owner/model-GGUF:Q5_K_M");
    fs::remove_dir_all(root)?;
    Ok(())
}

/// Removing a repository mirmir holds a download of keeps the cache of it
/// another tool keeps, which the listing does not show beside it.
#[tokio::test]
async fn a_repository_mirmir_holds_keeps_another_tools_cache() -> Result<()> {
    let root = std::env::temp_dir().join(format!("mirmir-external-held-{}", std::process::id()));
    drop(fs::remove_dir_all(&root));
    let mut paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let external = root.join("hf/hub/models--owner--lone-GGUF/snapshots/abc");
    fs::create_dir_all(&external)?;
    fs::write(external.join("model.gguf"), b"theirs")?;
    paths.external_hub_cache = Some(root.join("hf/hub"));
    let store = Store::new(paths);
    pending::begin(&store, &"owner/lone-GGUF:Q4_0".parse::<HubReference>()?, &pinned(), &[])?;
    let catalog = Catalog::new(store);
    assert!(catalog.remove("owner/lone-GGUF").await?.removed);
    assert!(crate::catalog::pending_variants(catalog.store.paths()).is_empty());
    assert_eq!(fs::read(external.join("model.gguf"))?, b"theirs");
    fs::remove_dir_all(root)?;
    Ok(())
}

/// Removing a repository with only an orphaned variant view deletes the
/// view and reports that something was removed.
#[tokio::test]
async fn an_orphaned_view_counts_as_removed() -> Result<()> {
    let root = std::env::temp_dir().join(format!("mirmir-orphan-view-{}", std::process::id()));
    drop(fs::remove_dir_all(&root));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let view = paths.gguf_variants_dir.join("owner--orphan-GGUF@Q4_0");
    fs::create_dir_all(&view)?;
    let catalog = Catalog::new(Store::new(paths));
    assert!(catalog.remove("owner/orphan-GGUF").await?.removed);
    assert!(!view.exists());
    fs::remove_dir_all(root)?;
    Ok(())
}
