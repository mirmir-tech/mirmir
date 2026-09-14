use super::*;

#[test]
fn auto_removes_overrides_from_inline_runtime_tables() -> Result<()> {
    let store = store();
    store.paths.ensure_config_dirs()?;
    fs::write(&store.paths.config_file, "# keep this\nschema_version = 1\nruntime = {}\n")?;
    for (key, setting) in [
        ("runtime.cached_prefill_policy", "interleave_one_block"),
        ("runtime.kv_blocks", "256"),
        ("runtime.kv_cache_dtype", "int8_per_token_head"),
    ] {
        store.set_config_value(key, setting)?;
        store.set_config_value(key, "auto")?;
        let text = fs::read_to_string(&store.paths.config_file)?;
        assert!(text.contains("# keep this"));
        let parsed: toml::Value = toml::from_str(&text)?;
        let field = key.strip_prefix("runtime.").ok_or_else(|| Error::Config("test key".into()))?;
        assert!(parsed.get("runtime").and_then(|runtime| runtime.get(field)).is_none());
    }
    Ok(())
}
