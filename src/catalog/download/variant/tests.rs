use std::fs;

use super::*;
use crate::config::Paths;

/// A cache of one repository: blobs hard-linked into a snapshot, and a view
/// per variant linking its text file and the shared vision tower.
fn cache(label: &str) -> Result<(Store, PathBuf, PathBuf)> {
    let root = std::env::temp_dir().join(format!("mirmir-variant-{label}-{}", std::process::id()));
    drop(fs::remove_dir_all(&root));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let repo = paths.hub_cache_dir.join("models--owner--model-GGUF");
    let snapshot = repo.join("snapshots/abc");
    fs::create_dir_all(repo.join("blobs"))?;
    fs::create_dir_all(&snapshot)?;
    for (blob, file, bytes) in
        [("b1", "m-Q4_0.gguf", 4), ("b2", "m-Q8_0.gguf", 8), ("b3", "mmproj-F16.gguf", 2)]
    {
        fs::write(repo.join("blobs").join(blob), vec![0_u8; bytes])?;
        fs::hard_link(repo.join("blobs").join(blob), snapshot.join(file))?;
    }
    let store = Store::new(paths);
    for (variant, file) in [("Q4_0", "m-Q4_0.gguf"), ("Q8_0", "m-Q8_0.gguf")] {
        let reference = format!("owner/model-GGUF:{variant}").parse::<HubReference>()?;
        downloaded(&store, &reference, &snapshot, &[file, "mmproj-F16.gguf"])?;
    }
    Ok((store, repo, root))
}

/// Links `files` in the variant's directory and records it downloaded.
fn downloaded(
    store: &Store,
    reference: &HubReference,
    snapshot: &Path,
    files: &[&str],
) -> Result<()> {
    let files = files.iter().map(|&file| file.to_owned()).collect::<Vec<_>>();
    let view = link(store, reference, snapshot, &files)?;
    store.save_hub_model(&reference.to_string(), "main", "abc", view)?;
    Ok(())
}

/// The key `name--Q4_0` of a repository without an owner starts like
/// `name--other--Q8_0`, a variant of `name/other`, which is no sibling.
#[test]
fn a_repository_without_an_owner_has_no_siblings_in_another() -> Result<()> {
    let (store, repo, root) = cache("ownerless")?;
    let snapshot = repo.join("snapshots/abc");
    let plain = "model-GGUF:Q4_0".parse::<HubReference>()?;
    downloaded(&store, &plain, &snapshot, &["m-Q4_0.gguf"])?;
    let other = "model-GGUF/other:Q8_0".parse::<HubReference>()?;
    downloaded(&store, &other, &snapshot, &["m-Q8_0.gguf"])?;
    assert!(siblings(&store, &plain)?.is_empty());
    assert_eq!(siblings(&store, &other)?.len(), 0);
    let owned = "owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    assert_eq!(siblings(&store, &owned)?.len(), 1);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn removing_a_variant_keeps_what_another_variant_links() -> Result<()> {
    let (store, repo, root) = cache("shared")?;
    let first = "owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    assert_eq!(siblings(&store, &first)?.len(), 1);
    // Its own text file goes, blob included; the shared tower stays.
    assert_eq!(remove_files(&store, &first)?, 4);
    assert!(!repo.join("blobs/b1").exists() && !repo.join("snapshots/abc/m-Q4_0.gguf").exists());
    assert!(repo.join("blobs/b3").exists() && repo.join("snapshots/abc/mmproj-F16.gguf").exists());
    assert!(siblings(&store, &first)?.len() == 1 && remove_files(&store, &first)? == 0);
    // The last variant takes the tower with it.
    store.remove_hub_model_config(&first.to_string())?;
    let last = "owner/model-GGUF:Q8_0".parse::<HubReference>()?;
    assert_eq!(remove_files(&store, &last)?, 10);
    assert!(siblings(&store, &last)?.is_empty());
    assert_eq!(fs::read_dir(repo.join("blobs"))?.count(), 0);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A variant downloading into the shared cache is pending until its
/// configuration is saved; removing it forgets it as well.
#[test]
fn a_downloading_variant_is_pending_until_it_finishes() -> Result<()> {
    let (store, _, root) = cache("pending")?;
    let variant = "owner/model-GGUF:Q6_K".parse::<HubReference>()?;
    assert!(pending_variants(store.paths()).is_empty());
    pending::begin(&store, &variant)?;
    assert_eq!(pending_variants(store.paths()), std::slice::from_ref(&variant));
    end_pending(&store, &variant)?;
    end_pending(&store, &variant)?;
    assert!(pending_variants(store.paths()).is_empty());
    fs::remove_dir_all(root)?;
    Ok(())
}
