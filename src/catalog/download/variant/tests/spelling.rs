//! A variant asked for by its label in another case than it was saved in,
//! as `Catalog::remove` takes it from a user.

use std::fs;

use super::{cache, pinned};
use crate::{
    catalog::{
        Catalog,
        download::variant::{link, pending, remove_files},
    },
    config::HubReference,
    error::Result,
};

/// Removing `owner/model-GGUF:q4_0` removes the installed `Q4_0` whole:
/// its view, its configuration and its own blob, and it is no longer
/// downloaded in either spelling.
#[tokio::test]
async fn removing_a_variant_by_another_spelling_removes_the_installed_one() -> Result<()> {
    let (store, repo, root) = cache("spelling-remove")?;
    let models = store.paths().models_dir.clone();
    let view = store.paths().gguf_variants_dir.join("owner--model-GGUF@Q4_0");
    let catalog = Catalog::new(store);
    let removal = catalog.remove("owner/model-GGUF:q4_0").await?;
    assert!(removal.removed);
    assert_eq!(removal.freed_bytes, 4);
    assert!(!view.exists() && !repo.join("blobs/b1").exists());
    assert!(!models.join("owner--model-GGUF@Q4_0.toml").exists());
    let store = &catalog.store;
    assert!(!store.hub_model_downloaded("owner/model-GGUF:Q4_0")?);
    assert!(!store.hub_model_downloaded("owner/model-GGUF:q4_0")?);
    // The other variant stays usable.
    assert!(store.hub_model_downloaded("owner/model-GGUF:Q8_0")?);
    let other = store.selected_model("owner/model-GGUF:Q8_0")?;
    assert_eq!(fs::read(other.path.join("m-Q8_0.gguf"))?.len(), 8);
    // A label nothing was saved under removes nothing.
    let unknown = catalog.remove("owner/model-GGUF:Q2_K").await?;
    assert!(!unknown.removed && unknown.freed_bytes == 0);
    assert!(store.hub_model_downloaded("owner/model-GGUF:Q8_0")?);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A variant downloading at the commit an installed one links its projector
/// from keeps that snapshot entry when the installed one goes, and its view,
/// published after, reads the projector.
#[test]
fn removing_a_variant_keeps_the_entry_a_download_claims() -> Result<()> {
    let (store, repo, root) = cache("in-flight")?;
    store.remove_hub_model_config("owner/model-GGUF:Q8_0")?;
    let downloading = "owner/model-GGUF:Q6_K".parse::<HubReference>()?;
    let file = |path: &str, etag: &str, size| crate::catalog::resumable::ListedFile {
        path: path.to_owned(),
        etag: etag.to_owned(),
        size,
    };
    let files = [file("m-Q6_K.gguf", "b4", 6), file("mmproj-F16.gguf", "b3", 2)];
    pending::begin(&store, &downloading, &pinned(), &files)?;
    let installed = "owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    assert_eq!(remove_files(&store, &installed)?, 4);
    let snapshot = repo.join("snapshots/abc");
    assert!(snapshot.join("mmproj-F16.gguf").exists() && repo.join("blobs/b3").exists());
    let view = link(&store, &downloading, &snapshot, &["mmproj-F16.gguf".to_owned()])?;
    assert_eq!(fs::read(view.join("mmproj-F16.gguf"))?.len(), 2);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// The repository's owner spelled otherwise is the same repository: the
/// removal takes the installed `Q4_0` alone and keeps the cache `Q8_0`
/// still reads its weights from.
#[tokio::test]
async fn removing_by_another_owner_spelling_keeps_the_other_variant() -> Result<()> {
    let (store, repo, root) = cache("owner-spelling")?;
    let catalog = Catalog::new(store);
    let removal = catalog.remove("Owner/model-GGUF:Q4_0").await?;
    assert!(removal.removed && removal.freed_bytes == 4);
    let store = &catalog.store;
    assert!(!store.hub_model_downloaded("owner/model-GGUF:Q4_0")?);
    assert!(repo.join("blobs/b2").exists() && repo.join("blobs/b3").exists());
    let other = store.selected_model("OWNER/Model-gguf:q8_0")?;
    assert_eq!(other.key, "owner--model-GGUF@Q8_0");
    assert_eq!(fs::read(other.path.join("m-Q8_0.gguf"))?.len(), 8);
    assert_eq!(fs::read(other.path.join("mmproj-F16.gguf"))?.len(), 2);
    // The last variant, spelled otherwise again, takes the cache with it.
    assert!(catalog.remove("OWNER/model-gguf:Q8_0").await?.removed);
    assert!(!repo.exists());
    fs::remove_dir_all(root)?;
    Ok(())
}
