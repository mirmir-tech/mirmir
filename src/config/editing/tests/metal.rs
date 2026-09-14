use super::*;

const KEY: &str = "runtime.metal_decode_reservation";

#[cfg(target_os = "macos")]
#[test]
fn metal_reservation_roundtrips_maps_and_resets_without_changing_the_default() -> Result<()> {
    use libmir::MetalDecodeReservation::{GenerationBudget, OnePage};
    let store = store();
    store.initialize()?;
    fs::write(&store.paths.config_file, "# preserve my configuration\nschema_version = 1\n")?;
    assert_eq!(
        store
            .load()?
            .runtime
            .to_libmir(&store.paths.state_dir)
            .metal
            .cache
            .decode_reservation,
        OnePage
    );
    check_presentation(&store, "auto", "default")?;
    for (input, policy) in [("generation_budget", GenerationBudget), ("one_page", OnePage)] {
        store.set_config_value(KEY, input)?;
        let runtime = store.load()?.runtime;
        assert_eq!(runtime.metal_decode_reservation, Some(policy));
        assert_eq!(
            runtime.to_libmir(&store.paths.state_dir).metal.cache.decode_reservation,
            policy
        );
        check_presentation(&store, input, "config.toml")?;
        assert!(
            fs::read_to_string(&store.paths.config_file)?.contains("# preserve my configuration")
        );
    }
    store.set_config_value(KEY, "auto")?;
    assert_eq!(store.load()?.runtime.metal_decode_reservation, None);
    assert_eq!(
        store
            .load()?
            .runtime
            .to_libmir(&store.paths.state_dir)
            .metal
            .cache
            .decode_reservation,
        OnePage
    );
    check_presentation(&store, "auto", "default")?;
    assert!(!fs::read_to_string(&store.paths.config_file)?.contains("metal_decode_reservation"));
    Ok(())
}

#[cfg(target_os = "macos")]
fn check_presentation(store: &Store, expected: &str, source: &str) -> Result<()> {
    let presentation = store.configuration()?;
    let entries = presentation.values.iter().filter(|entry| entry.key == KEY).collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value, expected);
    assert_eq!(entries[0].source, source);
    assert!(entries[0].editable && entries[0].restart_required);
    Ok(())
}

#[test]
fn invalid_or_unsupported_metal_reservation_does_not_overwrite_configuration() -> Result<()> {
    let store = store();
    store.initialize()?;
    let before = fs::read_to_string(&store.paths.config_file)?;
    for invalid in ["unbounded", "256", "GenerationBudget", "tokens"] {
        assert!(store.set_config_value(KEY, invalid).is_err());
        assert_eq!(fs::read_to_string(&store.paths.config_file)?, before);
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert!(store.set_config_value(KEY, "generation_budget").is_err());
        assert!(
            toml::from_str::<AppConfig>(
                "[runtime]\nmetal_decode_reservation = 'generation_budget'"
            )
            .is_err()
        );
    }
    Ok(())
}
