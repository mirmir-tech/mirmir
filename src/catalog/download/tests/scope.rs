//! What a removal may delete, and what refuses it.

use super::{root, *};
use crate::config::HeldCache;

/// Another tool's cache is a removal's only for its repository, and only
/// while mirmir holds nothing of it: no own copy, no configuration.
#[tokio::test]
async fn a_removal_takes_another_tools_cache_only_when_mirmir_holds_nothing() -> Result<()> {
    let root = root("scope-external");
    let mut paths = crate::config::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    );
    paths.external_hub_cache = Some(root.join("hf"));
    let external = root.join("hf/models--Owner--Ext");
    let snapshot = external.join("snapshots/abc");
    std::fs::create_dir_all(&snapshot)?;
    std::fs::write(snapshot.join("config.json"), b"{}")?;
    let own = paths.hub_cache_dir.join("models--owner--ext");
    let store = Store::new(paths);
    let kinds = |asked: &str| -> Result<Vec<CacheKind>> {
        let scope = removal_scope(&store, &asked.parse()?)?;
        Ok(scope.caches().iter().map(RepositoryCache::kind).collect())
    };
    assert_eq!(kinds("Owner/Ext:Q4_0")?, []);
    assert_eq!(kinds("owner/ext")?, [CacheKind::External]);
    std::fs::create_dir_all(&own)?;
    assert_eq!(kinds("owner/ext")?, [CacheKind::Managed]);
    std::fs::remove_dir_all(&own)?;
    store.save_hub_model("Owner/Ext", "main", "abc", snapshot.canonicalize()?)?;
    assert_eq!(kinds("owner/ext")?, []);
    // Its configuration goes, the other tool's files stay; then they go.
    assert!(removing(&store, "owner/ext").await?.removed);
    assert!(snapshot.exists());
    assert!(removing(&store, "owner/ext").await?.removed);
    assert!(!external.exists());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// A model configured under an id of its own within a cache a removal
/// would delete refuses the removal, naming it, and its files stay: in
/// another tool's cache, which no configuration of the repository holds,
/// and in mirmir's own, for the repository and for a variant of it.
#[tokio::test]
async fn a_model_configured_within_a_cache_refuses_its_removal() -> Result<()> {
    let root = root("scope-configured");
    let mut paths = crate::config::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    );
    paths.external_hub_cache = Some(root.join("hf"));
    let external = root.join("hf/models--Owner--Ext/snapshots/abc");
    let own = paths.hub_cache_dir.join("models--Owner--Own/snapshots/abc");
    for snapshot in [&external, &own] {
        std::fs::create_dir_all(snapshot)?;
        std::fs::write(snapshot.join("config.json"), b"{}")?;
    }
    let store = Store::new(paths);
    let configure = |snapshot: &std::path::Path, id: &str| -> Result<std::path::PathBuf> {
        let change = crate::config::ModelConfigChange { generation: None, runtime: None };
        store.save_model_config(&snapshot.display().to_string(), id, None, change)?;
        Ok(snapshot.canonicalize()?)
    };
    let refusal = |asked: &str, model: &str, path: &std::path::Path| -> Result<Error> {
        let held = HeldCache::new(asked.parse()?, model.to_owned(), path.to_path_buf());
        Ok(Error::CacheHeld(held))
    };
    let (foo, bar) = (configure(&external, "foo")?, configure(&own, "bar")?);
    for (asked, model, path) in
        [("owner/ext", "foo", &foo), ("owner/own", "bar", &bar), ("owner/own:Q4_0", "bar", &bar)]
    {
        let refused = removing(&store, asked).await.expect_err("the removal is refused");
        assert_eq!(refused.to_string(), refusal(asked, model, path)?.to_string());
        let class = crate::application::Error::from(refused).class();
        assert_eq!(class, crate::application::ErrorClass::Conflict);
    }
    assert!(external.join("config.json").exists() && own.join("config.json").exists());
    std::fs::remove_dir_all(root)?;
    Ok(())
}
