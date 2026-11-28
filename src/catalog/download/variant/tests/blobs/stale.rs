//! A listing of `blobs` that no longer holds: listed again rather than
//! trusted, and a listing that fails removes nothing.

use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    time::Duration,
};

use super::{super::cache, reads};
use crate::{
    catalog::download::variant::{Blobs, remove_files},
    config::HubReference,
    error::Result,
};

/// A blob renamed or removed since its repository was listed is listed
/// again: the lookup finds it under its new name, or finds none. A file
/// no blob was, unchanged since, finds none without listing again.
#[test]
fn a_blob_changed_since_it_was_listed_is_listed_again() -> Result<()> {
    let (_store, repo, root) = cache("stale")?;
    let snapshot = repo.join("snapshots/abc").canonicalize()?;
    let blob = |name: &str| repo.join("blobs").join(name).canonicalize();
    fs::write(snapshot.join("config.json"), b"{}")?;
    let mut blobs = Blobs::default();
    reads();
    assert_eq!(
        blobs.of(&snapshot.join("mmproj-F16.gguf"))?.map(|blob| blob.path),
        Some(blob("b3")?)
    );
    assert!(blobs.of(&snapshot.join("config.json"))?.is_none());
    assert_eq!(reads().1, 1);
    fs::rename(repo.join("blobs/b2"), repo.join("blobs/b9"))?;
    assert_eq!(
        blobs.of(&snapshot.join("m-Q8_0.gguf"))?.map(|blob| blob.path),
        Some(blob("b9")?)
    );
    fs::remove_file(repo.join("blobs/b1"))?;
    assert!(blobs.of(&snapshot.join("m-Q4_0.gguf"))?.is_none());
    assert_eq!(reads().1, 2);
    // A blob removed through the listing is no longer found.
    assert_eq!(blobs.remove(&blob("b3")?)?, 0);
    assert!(blobs.of(&snapshot.join("mmproj-F16.gguf"))?.is_none());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A blob another process links a snapshot file as after the listing is
/// found: the file changed since, so its repository is listed again.
#[test]
fn a_blob_linked_since_the_listing_is_found() -> Result<()> {
    let (_store, repo, root) = cache("linked-since")?;
    let snapshot = repo.join("snapshots/abc").canonicalize()?;
    fs::write(snapshot.join("m-late.gguf"), [0_u8; 3])?;
    let mut blobs = Blobs::default();
    assert!(blobs.of(&snapshot.join("m-late.gguf"))?.is_none());
    // Past the coarsest granule a file system stamps the change to.
    blobs.list_earlier(Duration::from_secs(10));
    fs::hard_link(snapshot.join("m-late.gguf"), repo.join("blobs/b7"))?;
    let found = blobs.of(&snapshot.join("m-late.gguf"))?.map(|blob| blob.path);
    assert_eq!(found, Some(repo.join("blobs/b7").canonicalize()?));
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A `blobs` that cannot be listed fails the removal before it takes the
/// entry whose blob it would leave behind; the failure is not kept, and
/// the same removal lists it again once it can.
#[test]
fn a_failed_listing_removes_nothing() -> Result<()> {
    let (store, repo, root) = cache("unlistable")?;
    let directory = repo.join("blobs");
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o300))?;
    // A superuser lists it anyway.
    let listable = fs::read_dir(&directory).is_ok();
    let variant = "owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    let mut blobs = Blobs::default();
    let failed = (!listable).then(|| remove_files(&store, &variant, &mut blobs));
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))?;
    if let Some(failed) = failed {
        assert!(failed.is_err());
        let entry = repo.join("snapshots/abc/m-Q4_0.gguf");
        assert!(entry.exists() && directory.join("b1").exists());
        assert_eq!(remove_files(&store, &variant, &mut blobs)?, 4);
        assert!(!directory.join("b1").exists() && !entry.exists());
    }
    fs::remove_dir_all(root)?;
    Ok(())
}

/// What is not a file in `blobs`, a link looping or naming nothing, or a
/// folder, is no blob and fails no removal.
#[test]
fn what_is_no_file_in_blobs_is_passed_over() -> Result<()> {
    let (store, repo, root) = cache("not-files")?;
    let directory = repo.join("blobs");
    symlink("loop", directory.join("loop"))?;
    symlink("gone", directory.join("dangling"))?;
    fs::create_dir(directory.join("folder"))?;
    let variant = "owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    assert_eq!(remove_files(&store, &variant, &mut Blobs::default())?, 4);
    assert!(!directory.join("b1").exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A hard link's blob goes before its entry: a blob that cannot go fails
/// the removal with its entry still naming it, and the removal tried again
/// takes both.
#[test]
fn a_blob_that_cannot_go_keeps_its_entry() -> Result<()> {
    let (store, repo, root) = cache("blob-first")?;
    let directory = repo.join("blobs");
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o555))?;
    // A superuser removes it anyway.
    let writable = fs::write(directory.join("probe"), b"").is_ok();
    let variant = "owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    let failed = (!writable).then(|| remove_files(&store, &variant, &mut Blobs::default()));
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))?;
    if let Some(failed) = failed {
        assert!(failed.is_err());
        let entry = repo.join("snapshots/abc/m-Q4_0.gguf");
        assert!(entry.exists() && directory.join("b1").exists());
        assert_eq!(remove_files(&store, &variant, &mut Blobs::default())?, 4);
        assert!(!entry.exists() && !directory.join("b1").exists());
    }
    fs::remove_dir_all(root)?;
    Ok(())
}
