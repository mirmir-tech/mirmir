//! Snapshots whose entries are links naming their blobs, as
//! `huggingface_hub` makes them: a removal takes the entry and the blob it
//! names, keeps a blob another commit's entry or a download claims, and
//! never a file outside the repository's `blobs`.

use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

use super::{
    super::{cache, downloaded, pinned},
    listed, names,
};
use crate::{
    catalog::download::variant::{Blobs, pending, remove_files, remove_pending},
    config::{HubReference, Store},
    error::Result,
};

/// The cache of `cache`, its snapshot's entries links naming its blobs.
fn linked(label: &str) -> Result<(Store, PathBuf, PathBuf)> {
    let (store, repo, root) = cache(label)?;
    for (blob, file) in [("b1", "m-Q4_0.gguf"), ("b2", "m-Q8_0.gguf"), ("b3", "mmproj-F16.gguf")] {
        let entry = repo.join("snapshots/abc").join(file);
        fs::remove_file(&entry)?;
        symlink(Path::new("../../blobs").join(blob), &entry)?;
    }
    Ok((store, repo, root))
}

/// Whether `path` is there, a link naming nothing included.
fn present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn reference(variant: &str) -> Result<HubReference> {
    format!("owner/model-GGUF:{variant}").parse()
}

/// A variant goes with its entry and its blob, leaving no link naming
/// nothing; the tower its sibling links stays until the last goes.
#[test]
fn a_linked_entry_goes_with_its_blob() -> Result<()> {
    let (store, repo, root) = linked("linked")?;
    let snapshot = repo.join("snapshots/abc");
    let mut blobs = Blobs::default();
    assert_eq!(remove_files(&store, &reference("Q4_0")?, &mut blobs)?, 4);
    assert!(!present(&repo.join("blobs/b1")) && !present(&snapshot.join("m-Q4_0.gguf")));
    assert_eq!(fs::read(snapshot.join("mmproj-F16.gguf"))?.len(), 2);
    store.remove_hub_model_config("owner/model-GGUF:Q4_0")?;
    assert_eq!(remove_files(&store, &reference("Q8_0")?, &mut blobs)?, 10);
    assert!(names(&repo.join("blobs"))?.is_empty() && names(&snapshot)?.is_empty());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A blob another commit's entry names, which a sibling links, stays with
/// that entry; the removed variant's own entry of it goes.
#[test]
fn a_blob_another_commit_links_stays() -> Result<()> {
    let (store, repo, root) = linked("linked-revisions")?;
    let other = repo.join("snapshots/def");
    fs::create_dir_all(&other)?;
    symlink("../../blobs/b2", other.join("m-Q8_0.gguf"))?;
    symlink("../../blobs/b3", other.join("mmproj-F16.gguf"))?;
    fs::remove_file(repo.join("snapshots/abc/m-Q8_0.gguf"))?;
    downloaded(&store, &reference("Q8_0")?, &other, &["m-Q8_0.gguf", "mmproj-F16.gguf"])?;
    assert_eq!(remove_files(&store, &reference("Q4_0")?, &mut Blobs::default())?, 4);
    assert!(!present(&repo.join("snapshots/abc/mmproj-F16.gguf")));
    assert_eq!(fs::read(other.join("mmproj-F16.gguf"))?.len(), 2);
    assert_eq!(names(&repo.join("blobs"))?, ["b2", "b3"]);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// The blob and the entry a downloading variant records stay for it to
/// publish, though no downloaded variant links them any more.
#[test]
fn a_linked_blob_a_download_claims_stays() -> Result<()> {
    let (store, repo, root) = linked("linked-claimed")?;
    store.remove_hub_model_config("owner/model-GGUF:Q8_0")?;
    let files = [listed("m-Q6_K.gguf", "b4", 6), listed("mmproj-F16.gguf", "b3", 2)];
    pending::begin(&store, &reference("Q6_K")?, &pinned(), &files)?;
    assert_eq!(remove_files(&store, &reference("Q4_0")?, &mut Blobs::default())?, 4);
    assert_eq!(fs::read(repo.join("snapshots/abc/mmproj-F16.gguf"))?.len(), 2);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// An entry linking a file outside the repository's `blobs` goes alone:
/// the file it names is not the repository's to remove.
#[test]
fn a_file_outside_the_cache_stays() -> Result<()> {
    let (store, repo, root) = linked("linked-outside")?;
    let outside = root.join("models/m-Q4_0.gguf");
    fs::create_dir_all(root.join("models"))?;
    fs::write(&outside, [0_u8; 4])?;
    let entry = repo.join("snapshots/abc/m-Q4_0.gguf");
    fs::remove_file(&entry)?;
    symlink(&outside, &entry)?;
    assert_eq!(remove_files(&store, &reference("Q4_0")?, &mut Blobs::default())?, 0);
    assert!(outside.exists() && !present(&entry));
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A blob another snapshot's link names stays with that link, though no
/// model links it: another revision's, or a snapshot mirmir lists without
/// a configuration of its own.
#[test]
fn a_blob_an_unconfigured_snapshot_links_stays() -> Result<()> {
    for (commit, file) in [("def", "m-Q4_0.gguf"), ("unconfigured", "model.gguf")] {
        let (store, repo, root) = linked(&format!("linked-{commit}"))?;
        let other = repo.join("snapshots").join(commit);
        fs::create_dir_all(&other)?;
        fs::write(other.join("config.json"), b"{}")?;
        symlink("../../blobs/b1", other.join(file))?;
        assert_eq!(remove_files(&store, &reference("Q4_0")?, &mut Blobs::default())?, 0);
        assert!(!present(&repo.join("snapshots/abc/m-Q4_0.gguf")));
        assert_eq!(fs::read(other.join(file))?.len(), 4, "{commit}");
        fs::remove_dir_all(root)?;
    }
    Ok(())
}

/// A downloading variant's finished blob another snapshot's link names
/// stays with that link; the download's own entry goes.
#[test]
fn a_downloaded_blob_another_snapshot_links_stays() -> Result<()> {
    let (store, repo, root) = linked("linked-pending")?;
    fs::write(repo.join("blobs/b4"), [0_u8; 6])?;
    symlink("../../blobs/b4", repo.join("snapshots/abc/m-Q6_K.gguf"))?;
    let other = repo.join("snapshots/def");
    fs::create_dir_all(&other)?;
    symlink("../../blobs/b4", other.join("m-Q6_K.gguf"))?;
    let variant = reference("Q6_K")?;
    pending::begin(&store, &variant, &pinned(), &[listed("m-Q6_K.gguf", "b4", 6)])?;
    assert_eq!(remove_pending(&store, &variant, &mut Blobs::default())?, 0);
    assert!(!present(&repo.join("snapshots/abc/m-Q6_K.gguf")));
    assert_eq!(fs::read(other.join("m-Q6_K.gguf"))?.len(), 6);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A repository whose `blobs` is a link to a folder elsewhere: a blob a
/// hard-linked entry is, which another revision's link names, stays; the
/// last variant frees the blobs no link names.
#[test]
fn blobs_linked_elsewhere_are_the_repositorys() -> Result<()> {
    let (store, repo, root) = cache("linked-blobs")?;
    let elsewhere = root.join("elsewhere");
    fs::rename(repo.join("blobs"), &elsewhere)?;
    symlink(&elsewhere, repo.join("blobs"))?;
    let other = repo.join("snapshots/def");
    fs::create_dir_all(&other)?;
    symlink("../../blobs/b1", other.join("m-Q4_0.gguf"))?;
    let mut blobs = Blobs::default();
    assert_eq!(remove_files(&store, &reference("Q4_0")?, &mut blobs)?, 0);
    assert!(!present(&repo.join("snapshots/abc/m-Q4_0.gguf")));
    assert_eq!(fs::read(other.join("m-Q4_0.gguf"))?.len(), 4);
    store.remove_hub_model_config("owner/model-GGUF:Q4_0")?;
    assert_eq!(remove_files(&store, &reference("Q8_0")?, &mut blobs)?, 10);
    assert_eq!(names(&elsewhere)?, ["b1"]);
    fs::remove_dir_all(root)?;
    Ok(())
}
