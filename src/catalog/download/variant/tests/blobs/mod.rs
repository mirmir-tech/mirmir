//! Finding the blobs of a removal's snapshot entries: the repository's
//! `blobs` listed once for every shard and view, and its bytes freed once.
//! Off Unix no file has an identity to find its blob by.

use std::{fs, path::Path};

use super::{cache, downloaded, pinned};
use crate::{
    catalog::{
        Catalog,
        download::variant::{Blobs, blobs::READS, pending, remove_files, remove_pending},
    },
    config::{HubReference, Store},
    error::Result,
};

mod links;
mod stale;

/// The metadata reads, `blobs` listings and snapshot walks since the last
/// call.
fn reads() -> (usize, usize, usize) {
    READS.with(std::cell::Cell::take)
}

/// Downloads `owner/model-GGUF:Q2_K` as `shards` one-byte shards, each a
/// blob of its own hard-linked into the snapshot, beside the shared tower.
fn sharded(store: &Store, repo: &Path, shards: usize) -> Result<HubReference> {
    let snapshot = repo.join("snapshots/abc");
    let mut files = vec!["mmproj-F16.gguf".to_owned()];
    for shard in 1..=shards {
        let file = format!("m-Q2_K-{shard:05}-of-{shards:05}.gguf");
        let blob = repo.join("blobs").join(format!("s{shard}"));
        fs::write(&blob, [0_u8])?;
        fs::hard_link(&blob, snapshot.join(&file))?;
        files.push(file);
    }
    let reference = "owner/model-GGUF:Q2_K".parse::<HubReference>()?;
    let files = files.iter().map(String::as_str).collect::<Vec<_>>();
    downloaded(store, &reference, &snapshot, &files)?;
    Ok(reference)
}

/// A file of a listing, as a download records it.
fn listed(path: &str, etag: &str, size: u64) -> crate::catalog::resumable::ListedFile {
    crate::catalog::resumable::ListedFile {
        path: path.to_owned(),
        etag: etag.to_owned(),
        size,
    }
}

/// The names in `directory`, sorted.
fn names(directory: &Path) -> Result<Vec<String>> {
    let entries = fs::read_dir(directory)?.flatten();
    let mut names = entries
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

/// The metadata reads removing a variant of `shards` shards takes, having
/// listed the repository's `blobs` once; its own blobs go, the text files
/// and the tower its siblings link stay.
fn removal_reads(shards: usize) -> Result<usize> {
    let (store, repo, root) = cache(&format!("sharded-{shards}"))?;
    let variant = sharded(&store, &repo, shards)?;
    reads();
    assert_eq!(remove_files(&store, &variant, &mut Blobs::default())?, shards as u64);
    let (metadata, listings, walks) = reads();
    assert_eq!((listings, walks), (1, 1));
    assert_eq!(names(&repo.join("blobs"))?, ["b1", "b2", "b3"]);
    fs::remove_dir_all(root)?;
    Ok(metadata)
}

/// Each shard costs the same few reads however many shards there are,
/// where a scan per shard would read the whole `blobs` again for each.
#[test]
fn a_sharded_variant_lists_its_blobs_once() -> Result<()> {
    let (fewer, more) = (removal_reads(64)?, removal_reads(128)?);
    assert!(more - fewer <= 8 * 64, "64 shards read {fewer} times, 128 shards {more} times");
    Ok(())
}

/// Removing a repository lists its `blobs` once for all of its variants'
/// views, and frees the tower they share once, with the last.
#[tokio::test]
async fn a_repository_removal_lists_its_blobs_once() -> Result<()> {
    let (store, repo, root) = cache("repository-reads")?;
    sharded(&store, &repo, 16)?;
    let catalog = Catalog::new(store);
    reads();
    let removal = catalog.remove("owner/model-GGUF").await?;
    let (_, listings, walks) = reads();
    assert_eq!((listings, walks), (1, 1));
    assert!(removal.removed && !repo.exists());
    assert_eq!(removal.freed_bytes, 4 + 8 + 2 + 16);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// Variants linking a cache mirmir does not own, which their removal
/// leaves: the views alone free their bytes, each once, and the tower
/// they share with the last, through the one listing.
#[tokio::test]
async fn a_removal_frees_a_foreign_caches_bytes_once() -> Result<()> {
    let (store, repo, root) = cache("foreign-reads")?;
    let foreign = root.join("hf/hub/models--owner--model-GGUF");
    fs::create_dir_all(root.join("hf/hub"))?;
    fs::rename(&repo, &foreign)?;
    let snapshot = foreign.join("snapshots/abc");
    for (variant, file) in [("Q4_0", "m-Q4_0.gguf"), ("Q8_0", "m-Q8_0.gguf")] {
        let reference = format!("owner/model-GGUF:{variant}").parse::<HubReference>()?;
        downloaded(&store, &reference, &snapshot, &[file, "mmproj-F16.gguf"])?;
    }
    sharded(&store, &foreign, 16)?;
    let catalog = Catalog::new(store);
    reads();
    let removal = catalog.remove("owner/model-GGUF").await?;
    let (_, listings, walks) = reads();
    assert_eq!((listings, walks), (1, 1));
    assert_eq!(removal.freed_bytes, 4 + 8 + 2 + 16);
    assert!(names(&foreign.join("blobs"))?.is_empty() && names(&snapshot)?.is_empty());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A variant downloading again while installed goes on one listing: its
/// download's blobs, then its view's, the tower its sibling links staying.
#[test]
fn a_downloading_and_installed_variant_goes_on_one_listing() -> Result<()> {
    let (store, repo, root) = cache("pending-and-view")?;
    let variant = "owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    let snapshot = repo.join("snapshots/abc");
    fs::write(repo.join("blobs/b5"), [0_u8; 5])?;
    fs::hard_link(repo.join("blobs/b5"), snapshot.join("m-Q4_0-new.gguf"))?;
    let files = [listed("m-Q4_0-new.gguf", "b5", 5), listed("mmproj-F16.gguf", "b3", 2)];
    pending::begin(&store, &variant, &pinned(), &files)?;
    let mut blobs = Blobs::default();
    reads();
    assert_eq!(remove_pending(&store, &variant, &mut blobs)?, 5);
    assert_eq!(remove_files(&store, &variant, &mut blobs)?, 4);
    let (_, listings, walks) = reads();
    assert_eq!((listings, walks), (1, 1));
    assert_eq!(names(&repo.join("blobs"))?, ["b2", "b3"]);
    assert_eq!(names(&snapshot)?, ["m-Q8_0.gguf", "mmproj-F16.gguf"]);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A downloading variant's entry a sibling links stays, whatever blob the
/// download recorded for it.
#[test]
fn a_downloading_entry_a_sibling_links_stays() -> Result<()> {
    let (store, repo, root) = cache("pending-entry")?;
    let variant = "owner/model-GGUF:Q6_K".parse::<HubReference>()?;
    pending::begin(&store, &variant, &pinned(), &[listed("mmproj-F16.gguf", "b9", 2)])?;
    assert_eq!(remove_pending(&store, &variant, &mut Blobs::default())?, 0);
    assert_eq!(fs::read(repo.join("snapshots/abc/mmproj-F16.gguf"))?.len(), 2);
    fs::remove_dir_all(root)?;
    Ok(())
}
