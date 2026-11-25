use super::*;
use crate::config::{Paths, Store};

fn catalog(label: &str) -> (Catalog, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("mirmir-{label}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    (Catalog::new(Store::new(paths)), root)
}

/// A pull registered under its repository also holds the variant it chose:
/// that row reports downloading, a second pull of it is refused, and both
/// are forgotten when the pull ends.
#[test]
fn a_chosen_variant_transfers_under_its_own_id() -> Result<()> {
    let (catalog, _) = catalog("transfer");
    let repository = catalog.reserve("owner/model-GGUF", Activity::Pull)?;
    let chosen = catalog.reserve("owner/model-GGUF:Q8_0", Activity::Pull)?;
    assert!(catalog.is_transferring("owner/model-GGUF:Q8_0")?);
    assert!(catalog.reserve("owner/model-GGUF:Q8_0", Activity::Pull).is_err());
    // The variant is one model in any case its label is asked in.
    assert!(catalog.reserve("owner/model-GGUF:q8_0", Activity::Removal).is_err());
    assert!(catalog.reserve("owner/model-GGUF:q8_0", Activity::Pull).is_err());
    drop((chosen, repository));
    assert!(!catalog.is_transferring("owner/model-GGUF:Q8_0")?);
    assert!(!catalog.is_transferring("owner/model-GGUF")?);
    Ok(())
}

/// Removing a repository while one of its variants downloads is refused
/// and leaves the variant's partial blob in the shared cache.
#[tokio::test]
async fn a_repository_is_not_removed_under_a_variant_download() -> Result<()> {
    let (catalog, root) = catalog("remove-during-pull");
    let blobs = catalog.store.paths().hub_cache_dir.join("models--owner--model/blobs");
    std::fs::create_dir_all(&blobs)?;
    std::fs::write(blobs.join("blob.incomplete"), b"partial")?;
    let pull = catalog.reserve("owner/model:Q8_0", Activity::Pull)?;
    let refused = catalog.remove("owner/model").await;
    assert!(refused.is_err_and(|error| error.to_string().contains("downloading")));
    assert_eq!(std::fs::read(blobs.join("blob.incomplete"))?, b"partial");
    // Another variant's removal does not touch the downloading one.
    drop(catalog.reserve("owner/model:Q4_0", Activity::Removal)?);
    drop(pull);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// A variant pull starting while its repository is being removed is
/// refused before any request, as is a pull of the whole repository while
/// a variant is removed.
#[tokio::test]
async fn no_pull_starts_while_its_repository_is_removed() -> Result<()> {
    let (catalog, _) = catalog("pull-during-remove");
    let memory = MachineMemory {
        total: None,
        available: None,
        budget: None,
        source: "test",
    };
    let removal = catalog.reserve("owner/model", Activity::Removal)?;
    let (updates, _received) = tokio::sync::mpsc::channel(8);
    let cancellation = CancellationToken::new();
    let refused = catalog.pull("owner/model:Q8_0", None, memory, updates, &cancellation).await;
    assert!(refused.is_err_and(|error| error.to_string().contains("being removed")));
    drop(removal);
    let removal = catalog.reserve("owner/model:Q8_0", Activity::Removal)?;
    assert!(catalog.reserve("owner/model", Activity::Pull).is_err());
    assert!(catalog.reserve("other/model", Activity::Pull).is_ok());
    drop(removal);
    Ok(())
}
