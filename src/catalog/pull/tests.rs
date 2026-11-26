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

/// Removing a repository, or another of its variants, while one of its
/// variants downloads is refused and leaves the variant's partial blob in
/// the shared cache.
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
    // Nor is another variant's, whose files the downloading one may share.
    let refused = catalog.remove("owner/model:Q4_0").await;
    assert!(refused.is_err_and(|error| error.to_string().contains("owner/model:Q8_0")));
    assert!(catalog.reserve("owner/model:Q4_0", Activity::Removal).is_err());
    drop(pull);
    assert!(catalog.reserve("owner/model:Q4_0", Activity::Removal).is_ok());
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

/// A pull asked by the variant's file stem holds its label as soon as the
/// listing resolves it: removing the variant by its label is refused and
/// the partial blob it downloads stays.
#[tokio::test]
async fn a_pull_by_file_stem_holds_the_variants_label() -> Result<()> {
    let (catalog, root) = catalog("pull-by-stem");
    let blobs = catalog.store.paths().hub_cache_dir.join("models--owner--m/blobs");
    std::fs::create_dir_all(&blobs)?;
    std::fs::write(blobs.join("q8.incomplete"), b"partial")?;
    let files = ["m-Q4_0.gguf", "m-Q8_0.gguf"].map(|path| ListedFile {
        path: path.to_owned(),
        etag: path.to_owned(),
        size: 8,
    });
    let stem = "owner/m:m-Q8_0".parse::<HubReference>()?;
    let _pull = catalog.reserve(&stem.to_string(), Activity::Pull)?;
    let Some((label, _held)) = catalog.reserve_label(&stem, &files)? else {
        return Err(crate::error::Error::Config("the stem took no label".to_owned()));
    };
    assert_eq!(label.to_string(), "owner/m:Q8_0");
    let refused = catalog.remove("owner/m:Q8_0").await;
    assert!(refused.is_err_and(|error| error.to_string().contains("downloading")));
    assert_eq!(std::fs::read(blobs.join("q8.incomplete"))?, b"partial");
    // The label asked in another case is the same reservation, not another.
    let lower = "owner/m:q8_0".parse::<HubReference>()?;
    assert!(catalog.reserve_label(&lower, &files)?.is_none());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// A pull queued behind another transfer, during which its model is
/// installed (by the pull holding the slot, or another process), refuses
/// once it has the slot, before any request.
#[tokio::test]
async fn a_queued_pull_rechecks_the_installation() -> Result<()> {
    let (catalog, root) = catalog("queued-installed");
    let memory = MachineMemory {
        total: None,
        available: None,
        budget: None,
        source: "test",
    };
    let cancellation = CancellationToken::new();
    // Another transfer holds the only slot.
    let slot = catalog.transfer_queue.acquire(&cancellation).await?;
    let (updates, mut received) = tokio::sync::mpsc::channel(8);
    let queued = catalog.pull("owner/m:q4_k_m", None, memory, updates, &cancellation);
    let installed = async {
        // The pull is queued once it says so.
        let queued = received.recv().await.map(|update| update.phase);
        assert_eq!(queued, Some(TransferPhase::Queued));
        let view = catalog.store.paths().gguf_variants_dir.join("owner--m@Q4_K_M");
        std::fs::create_dir_all(&view)?;
        catalog.store.save_hub_model("owner/m:Q4_K_M", "main", "abc", view)?;
        drop(slot);
        Ok::<_, crate::error::Error>(())
    };
    let (refused, installed) = tokio::join!(queued, installed);
    installed?;
    assert!(refused.is_err_and(|error| error.to_string().contains("already downloaded")));
    std::fs::remove_dir_all(root)?;
    Ok(())
}
