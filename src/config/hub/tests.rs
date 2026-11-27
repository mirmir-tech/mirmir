use std::path::Path;

use super::*;

#[test]
fn reads_a_repository_and_its_variant() -> Result<()> {
    let reference = "unsloth/gemma-4-E4B-it-GGUF:Q4_K_M".parse::<HubReference>()?;
    assert_eq!(reference.repository(), "unsloth/gemma-4-E4B-it-GGUF");
    assert_eq!(reference.variant(), Some("Q4_K_M"));
    assert_eq!(reference.key(), "unsloth--gemma-4-E4B-it-GGUF@Q4_K_M");
    assert_eq!(reference.to_string(), "unsloth/gemma-4-E4B-it-GGUF:Q4_K_M");
    let plain = "Qwen/Qwen3-4B".parse::<HubReference>()?;
    assert_eq!((plain.variant(), plain.key().as_str()), (None, "Qwen--Qwen3-4B"));
    Ok(())
}

#[test]
fn refuses_traversal_and_keys_that_could_collide() {
    for value in ["../secret", "owner/model/extra", "owner/model:", "owner/model:a/b", "a/b:x--y"] {
        assert!(value.parse::<HubReference>().is_err(), "{value}");
    }
}

/// A removal owns its models' keys, and the models loaded by their paths
/// from within the caches it may delete, in any case; never a path in a
/// cache it leaves, nor a folder outside every cache named alike.
#[test]
fn a_removal_owns_what_its_caches_hold() -> Result<()> {
    let cache = |path: &str, kind| RepositoryCache::new(Path::new(path), Path::new("/c"), kind);
    let asked = "owner/repo:Q4_K_M".parse::<HubReference>()?;
    let scope = RemovalScope::new(asked, vec![cache("/c/models--Owner--Repo", CacheKind::Managed)]);
    for owned in [
        "Owner--Repo@q4_k_m",
        "/c/models--Owner--Repo/snapshots/abc",
        "/c/models--owner--repo/blobs/0123",
    ] {
        assert!(scope.owns_model(owned), "{owned}");
    }
    for other in [
        "Owner--Repo@Q8_0",
        "/c/models--Owner--Repo2/snapshots/abc",
        "/elsewhere/models--Owner--Repo/snapshots/abc",
        "/home/user/models--Owner--Repo",
        "models--Owner--Repo/snapshots/abc",
    ] {
        assert!(!scope.owns_model(other), "{other}");
    }
    assert_eq!(scope.asked().cache_directory(), "models--owner--repo");
    Ok(())
}
