use std::fs;

use super::{remove::siblings, *};
use crate::config::Paths;

fn pinned() -> crate::catalog::resumable::Pinned {
    crate::catalog::resumable::Pinned {
        requested: "main".to_owned(),
        commit: "abc".to_owned(),
    }
}

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

/// A variant of a repository without an owner, `name:Q4_0`, is no sibling
/// of `name/other:Q8_0`, a variant of another repository.
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

/// The repository's own model, saved at the snapshot a variant links,
/// keeps every file of it when the variant goes.
#[test]
fn removing_a_variant_keeps_the_base_snapshot() -> Result<()> {
    let (store, repo, root) = cache("base")?;
    let snapshot = repo.join("snapshots/abc");
    store.save_hub_model("owner/model-GGUF", "main", "abc", snapshot.clone())?;
    let variant = "owner/model-GGUF:Q4_0".parse::<HubReference>()?;
    store.remove_hub_model_config("owner/model-GGUF:Q8_0")?;
    assert_eq!(remove_files(&store, &variant)?, 0);
    for (blob, file) in [("b1", "m-Q4_0.gguf"), ("b2", "m-Q8_0.gguf"), ("b3", "mmproj-F16.gguf")] {
        assert!(repo.join("blobs").join(blob).exists() && snapshot.join(file).exists(), "{file}");
    }
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A variant saved under its legacy key, removed before anything listed
/// it, is removed with its view.
#[tokio::test]
async fn a_legacy_variant_is_removed_as_the_first_operation() -> Result<()> {
    let (store, _, root) = cache("legacy-remove")?;
    let variants = &store.paths().gguf_variants_dir;
    let (old, view) = (
        variants.join("owner--model-GGUF--Q4_0"),
        variants.join("owner--model-GGUF@Q4_0"),
    );
    fs::rename(&view, &old)?;
    let models = &store.paths().models_dir;
    let config = fs::read_to_string(models.join("owner--model-GGUF@Q4_0.toml"))?;
    fs::remove_file(models.join("owner--model-GGUF@Q4_0.toml"))?;
    fs::write(
        models.join("owner--model-GGUF--Q4_0.toml"),
        config.replace("owner--model-GGUF@Q4_0", "owner--model-GGUF--Q4_0"),
    )?;
    let removal = crate::catalog::download::remove(&store, "owner/model-GGUF:Q4_0").await?;
    assert!(removal.removed && removal.freed_bytes > 0);
    assert!(!old.exists() && !view.exists());
    assert!(!store.hub_model_downloaded("owner/model-GGUF:Q4_0")?);
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
    pending::begin(&store, &variant, &pinned(), &[])?;
    assert_eq!(pending_variants(store.paths()), std::slice::from_ref(&variant));
    end_pending(&store, &variant)?;
    end_pending(&store, &variant)?;
    assert!(pending_variants(store.paths()).is_empty());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A download interrupted before variant keys took `@` left its marker at
/// `owner--name--VARIANT`: it still counts its bytes and removing the
/// variant forgets it.
#[test]
fn a_legacy_pending_marker_is_the_variants_own() -> Result<()> {
    let (store, _, root) = cache("legacy-pending")?;
    let variant = "owner/model-GGUF:Q6_K".parse::<HubReference>()?;
    pending::begin(&store, &variant, &pinned(), &[])?;
    let variants = &store.paths().gguf_variants_dir;
    let legacy = variants.join("owner--model-GGUF--Q6_K.pending");
    fs::rename(variants.join("owner--model-GGUF@Q6_K.pending"), &legacy)?;
    assert_eq!(pending_variants(store.paths()), std::slice::from_ref(&variant));
    assert_eq!(pending::pending_bytes(store.paths(), &variant), Some(0));
    assert!(!legacy.exists());
    end_pending(&store, &variant)?;
    assert!(pending_variants(store.paths()).is_empty());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A repository holds more than a variant while another variant is
/// downloading or its own model is saved.
#[test]
fn a_pending_sibling_or_the_base_model_keeps_the_repository() -> Result<()> {
    let (store, _, root) = cache("holds")?;
    let lone = "owner/lone-GGUF:Q8_0".parse::<HubReference>()?;
    assert!(!holds_more(&store, &lone)?);
    pending::begin(&store, &"owner/lone-GGUF:Q6_K".parse::<HubReference>()?, &pinned(), &[])?;
    assert!(holds_more(&store, &lone)?);
    let base = "owner/base:Q8_0".parse::<HubReference>()?;
    assert!(!holds_more(&store, &base)?);
    store.save_hub_model("owner/base", "main", "abc", root.clone())?;
    assert!(holds_more(&store, &base)?);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A downloading variant beside downloaded siblings counts and removes its
/// own blobs: its partial text shard goes, the tower its siblings link stays.
#[test]
fn a_pending_variant_owns_its_partial_blobs() -> Result<()> {
    let (store, repo, root) = cache("own-blobs")?;
    let variant = "owner/model-GGUF:Q6_K".parse::<HubReference>()?;
    fs::write(repo.join("blobs/b4.incomplete"), [0_u8; 3])?;
    let file = |path: &str, etag: &str, size| crate::catalog::resumable::ListedFile {
        path: path.to_owned(),
        etag: etag.to_owned(),
        size,
    };
    let files = [file("m-Q6_K.gguf", "b4", 6), file("mmproj-F16.gguf", "b3", 2)];
    pending::begin(&store, &variant, &pinned(), &files)?;
    assert_eq!(pending_bytes(store.paths(), &variant), Some(5));
    assert_eq!(remove_pending(&store, &variant)?, 3);
    assert!(!repo.join("blobs/b4.incomplete").exists());
    assert!(repo.join("blobs/b3").exists() && repo.join("snapshots/abc/mmproj-F16.gguf").exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// Two interrupted variants sharing a partial vision tower: removing one
/// keeps the tower's partial blob, so the other resumes from its offset.
#[test]
fn removing_a_pending_variant_keeps_what_another_one_resumes() -> Result<()> {
    let (store, repo, root) = cache("shared-pending")?;
    let file = |path: &str, etag: &str, size| crate::catalog::resumable::ListedFile {
        path: path.to_owned(),
        etag: etag.to_owned(),
        size,
    };
    let tower = file("mmproj-BF16.gguf", "b6", 8);
    let first = "owner/model-GGUF:Q5_K_M".parse::<HubReference>()?;
    let second = "owner/model-GGUF:Q3_K_M".parse::<HubReference>()?;
    pending::begin(&store, &first, &pinned(), &[file("m-Q5.gguf", "b7", 4), tower.clone()])?;
    pending::begin(&store, &second, &pinned(), &[file("m-Q3.gguf", "b8", 4), tower])?;
    fs::write(repo.join("blobs/b6.incomplete"), [0_u8; 6])?;
    fs::write(repo.join("blobs/b7.incomplete"), [0_u8; 2])?;
    assert_eq!(pending_bytes(store.paths(), &second), Some(6));
    assert_eq!(remove_pending(&store, &first)?, 2);
    assert!(!repo.join("blobs/b7.incomplete").exists());
    assert_eq!(fs::metadata(repo.join("blobs/b6.incomplete"))?.len(), 6);
    assert_eq!(pending_bytes(store.paths(), &second), Some(6));
    fs::remove_dir_all(root)?;
    Ok(())
}
