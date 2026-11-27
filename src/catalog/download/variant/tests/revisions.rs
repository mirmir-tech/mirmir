//! Variants of one repository downloaded at different commits.

use std::fs;

use super::{cache, downloaded};
use crate::{
    catalog::download::variant::{Blobs, remove_files},
    config::HubReference,
    error::Result,
};

/// Variants pinned to different commits link one projector blob from two
/// snapshots: removing the first frees only its text file and leaves the
/// blob to the other, whose removal then frees it.
#[test]
fn a_blob_linked_from_another_commit_is_kept() -> Result<()> {
    let (store, repo, root) = cache("revisions")?;
    let other = repo.join("snapshots/def");
    fs::create_dir_all(&other)?;
    fs::hard_link(repo.join("blobs/b2"), other.join("m-Q8_0.gguf"))?;
    fs::hard_link(repo.join("blobs/b3"), other.join("mmproj-F16.gguf"))?;
    fs::remove_file(repo.join("snapshots/abc/m-Q8_0.gguf"))?;
    let last = "owner/model-GGUF:Q8_0".parse::<HubReference>()?;
    downloaded(&store, &last, &other, &["m-Q8_0.gguf", "mmproj-F16.gguf"])?;
    let first = "owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    // One index across both, as a repository's removal takes them.
    let mut blobs = Blobs::default();
    assert_eq!(remove_files(&store, &first, &mut blobs)?, 4);
    assert!(repo.join("blobs/b3").exists() && other.join("mmproj-F16.gguf").exists());
    assert!(!repo.join("snapshots/abc/mmproj-F16.gguf").exists());
    store.remove_hub_model_config(&first.to_string())?;
    assert_eq!(remove_files(&store, &last, &mut blobs)?, 10);
    assert_eq!(fs::read_dir(repo.join("blobs"))?.count(), 0);
    fs::remove_dir_all(root)?;
    Ok(())
}
