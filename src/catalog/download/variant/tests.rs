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
        link(&store, &reference, &snapshot, &[file.to_owned(), "mmproj-F16.gguf".to_owned()])?;
    }
    Ok((store, repo, root))
}

#[test]
fn removing_a_variant_keeps_what_another_variant_links() -> Result<()> {
    let (store, repo, root) = cache("shared")?;
    let first = "owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    assert_eq!(siblings(&store, &first).len(), 1);
    // Its own text file goes, blob included; the shared tower stays.
    assert_eq!(remove_files(&store, &first)?, 4);
    assert!(!repo.join("blobs/b1").exists() && !repo.join("snapshots/abc/m-Q4_0.gguf").exists());
    assert!(repo.join("blobs/b3").exists() && repo.join("snapshots/abc/mmproj-F16.gguf").exists());
    assert!(siblings(&store, &first).len() == 1 && remove_files(&store, &first)? == 0);
    // The last variant takes the tower with it.
    let last = "owner/model-GGUF:Q8_0".parse::<HubReference>()?;
    assert_eq!(remove_files(&store, &last)?, 10);
    assert!(siblings(&store, &last).is_empty());
    assert_eq!(fs::read_dir(repo.join("blobs"))?.count(), 0);
    fs::remove_dir_all(root)?;
    Ok(())
}
