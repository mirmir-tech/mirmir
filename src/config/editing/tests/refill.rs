use super::*;

#[test]
fn cached_prefill_policy_is_opt_in_typed_and_validated_before_write() -> Result<()> {
    use libmir::CachedPrefillPolicy::{BackendDefault, InterleaveOneBlock};
    let store = store();
    store.initialize()?;
    let runtime = store.load()?.runtime;
    assert_eq!(
        runtime.to_libmir(std::path::Path::new("/tmp")).scheduler.cached_prefill_policy,
        BackendDefault
    );
    store.set_config_value("runtime.cached_prefill_policy", "interleave_one_block")?;
    let runtime = store.load()?.runtime;
    assert_eq!(runtime.cached_prefill_policy, Some(InterleaveOneBlock));
    assert_eq!(
        runtime.to_libmir(std::path::Path::new("/tmp")).scheduler.cached_prefill_policy,
        InterleaveOneBlock
    );
    let before = fs::read_to_string(&store.paths.config_file)?;
    assert!(store.set_config_value("runtime.cached_prefill_policy", "unbounded").is_err());
    assert_eq!(fs::read_to_string(&store.paths.config_file)?, before);
    store.set_config_value("runtime.cached_prefill_policy", "auto")?;
    assert_eq!(store.load()?.runtime.cached_prefill_policy, None);
    assert_eq!(
        store
            .load()?
            .runtime
            .to_libmir(std::path::Path::new("/tmp"))
            .scheduler
            .cached_prefill_policy,
        BackendDefault
    );
    Ok(())
}

#[test]
fn prefill_decode_policy_is_opt_in_typed_and_validated_before_write() -> Result<()> {
    use libmir::PrefillDecodePolicy::{BackendDefault, CompleteCohort};
    let store = store();
    store.initialize()?;
    let runtime = store.load()?.runtime;
    assert_eq!(
        runtime.to_libmir(std::path::Path::new("/tmp")).scheduler.prefill_decode_policy,
        BackendDefault
    );
    store.set_config_value("runtime.prefill_decode_policy", "complete_cohort")?;
    let runtime = store.load()?.runtime;
    assert_eq!(runtime.prefill_decode_policy, Some(CompleteCohort));
    assert_eq!(
        runtime.to_libmir(std::path::Path::new("/tmp")).scheduler.prefill_decode_policy,
        CompleteCohort
    );
    let before = fs::read_to_string(&store.paths.config_file)?;
    assert!(store.set_config_value("runtime.prefill_decode_policy", "unbounded").is_err());
    assert_eq!(fs::read_to_string(&store.paths.config_file)?, before);
    store.set_config_value("runtime.prefill_decode_policy", "auto")?;
    assert_eq!(store.load()?.runtime.prefill_decode_policy, None);
    assert_eq!(
        store
            .load()?
            .runtime
            .to_libmir(std::path::Path::new("/tmp"))
            .scheduler
            .prefill_decode_policy,
        BackendDefault
    );
    Ok(())
}
