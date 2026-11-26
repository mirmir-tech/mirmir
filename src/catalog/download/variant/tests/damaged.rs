//! A configuration file that cannot be read, beside the installed models.

use std::fs;

use super::cache;
use crate::{catalog::Catalog, config::HubReference, error::Result};

/// An unrelated damaged configuration keeps no lookup, listing or bulk
/// index from finding the models that can be read.
#[test]
fn a_damaged_configuration_keeps_other_models_found() -> Result<()> {
    let (store, _, root) = cache("damaged-lookup")?;
    fs::write(store.paths().models_dir.join("someone--else.toml"), "path = [\n")?;
    assert!(!store.hub_model_downloaded("owner/model-GGUF:Q5_K_M")?);
    assert!(store.hub_model_downloaded("owner/model-GGUF:q4_0")?);
    let downloaded = store.downloaded_models()?;
    for variant in ["Q4_0", "Q8_0"] {
        let reference = format!("owner/model-GGUF:{variant}").parse::<HubReference>()?;
        assert!(downloaded.contains(&reference), "{variant}");
    }
    assert_eq!(store.list_model_configs()?.len(), 2);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A damaged configuration that may be a model of the repository stops its
/// removal before anything goes, naming the file; an unrelated one does
/// not.
#[tokio::test]
async fn a_damaged_configuration_of_the_repository_stops_its_removal() -> Result<()> {
    let (store, repo, root) = cache("damaged-removal")?;
    let damaged = store.paths().models_dir.join("owner--model-GGUF@Q2_K.toml");
    fs::write(&damaged, "path = [\n")?;
    fs::write(store.paths().models_dir.join("someone--else.toml"), "path = [\n")?;
    let catalog = Catalog::new(store);
    let refused = catalog.remove("owner/model-GGUF:Q4_0").await;
    assert!(refused.is_err_and(|error| error.to_string().contains("owner--model-GGUF@Q2_K.toml")));
    assert!(repo.join("blobs/b1").exists());
    assert!(catalog.store.hub_model_downloaded("owner/model-GGUF:Q4_0")?);
    fs::remove_file(damaged)?;
    assert!(catalog.remove("owner/model-GGUF:Q4_0").await?.removed);
    assert!(!repo.join("blobs/b1").exists() && repo.join("blobs/b2").exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// Damaged files keyed as the repository's in another spelling or as a
/// legacy variant, or a damaged download marker of it, stop its removal
/// too; the removed variant's own damaged configuration does not, and goes
/// with it.
#[tokio::test]
async fn every_damaged_file_of_the_repository_stops_its_removal() -> Result<()> {
    let (store, repo, root) = cache("damaged-spellings")?;
    let models = store.paths().models_dir.clone();
    let markers = store.paths().gguf_variants_dir.clone();
    let catalog = Catalog::new(store);
    for damaged in [
        models.join("Owner--model-gguf@Q2_K.toml"),
        models.join("owner--model-GGUF--Q3_K_M.toml"),
        markers.join("owner--model-GGUF@Q6_K.pending"),
    ] {
        fs::write(&damaged, "{ path = [\n")?;
        let refused = catalog.remove("owner/model-GGUF:Q4_0").await;
        let name = damaged.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        assert!(refused.is_err_and(|error| error.to_string().contains(name)), "{name}");
        assert!(repo.join("blobs/b1").exists(), "{name}");
        fs::remove_file(damaged)?;
    }
    let own = models.join("owner--model-GGUF@Q4_0.toml");
    fs::write(&own, "path = [\n")?;
    assert!(!catalog.store.hub_model_downloaded("owner/model-GGUF:Q4_0")?);
    assert!(catalog.remove("owner/model-GGUF:Q4_0").await?.removed);
    assert!(!own.exists() && !repo.join("blobs/b1").exists());
    assert!(catalog.store.hub_model_downloaded("owner/model-GGUF:Q8_0")?);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A variant's own configuration that cannot be read, spelled otherwise
/// than asked, goes with its view; and a repository whose own
/// configuration cannot be read is removed with it.
#[tokio::test]
async fn a_models_own_damaged_files_go_with_it() -> Result<()> {
    let (store, repo, root) = cache("damaged-own")?;
    let models = store.paths().models_dir.clone();
    // On a case-insensitive file system this is the installed file itself.
    fs::write(models.join("Owner--model-GGUF@Q4_0.toml"), "path = [\n")?;
    let catalog = Catalog::new(store);
    assert!(catalog.remove("owner/model-GGUF:q4_0").await?.removed);
    let left = fs::read_dir(&models)?
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok());
    assert!(
        left.into_iter()
            .all(|name| !name.eq_ignore_ascii_case("owner--model-GGUF@Q4_0.toml"))
    );
    assert!(!catalog.store.paths().gguf_variants_dir.join("owner--model-GGUF@Q4_0").exists());
    assert!(catalog.store.hub_model_downloaded("owner/model-GGUF:Q8_0")?);
    // Its legacy key, unreadable, is its own too: no Hub name holds `--`.
    fs::write(models.join("owner--model-GGUF--Q8_0.toml"), "path = [\n")?;
    assert!(catalog.remove("owner/model-GGUF:Q8_0").await?.removed);
    assert!(!models.join("owner--model-GGUF--Q8_0.toml").exists());
    fs::write(models.join("owner--model-GGUF.toml"), "path = [\n")?;
    fs::write(models.join("owner--model-GGUF--Q6_K.toml"), "path = [\n")?;
    assert!(catalog.remove("owner/model-GGUF").await?.removed);
    assert!(fs::read_dir(&models)?.next().is_none() && !repo.exists());
    fs::remove_dir_all(root)?;
    Ok(())
}
