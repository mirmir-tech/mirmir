use super::pull_ended_without_install;

/// Only a failed, cancelled or unreported pull releases a kept variant; a
/// completed one installed it.
#[test]
fn only_a_pull_that_installed_nothing_releases() {
    for state in [None, Some("failed"), Some("cancelled")] {
        assert!(pull_ended_without_install(state), "{state:?}");
    }
    for state in ["completed", "queued", "running", "cancelling"] {
        assert!(!pull_ended_without_install(Some(state)), "{state}");
    }
}

#[cfg(target_arch = "wasm32")]
mod browser;
