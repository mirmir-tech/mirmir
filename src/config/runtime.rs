use std::path::Path;
#[cfg(target_os = "linux")]
use std::{ffi::OsString, path::PathBuf};

use super::schema::RuntimeSettings;

impl RuntimeSettings {
    #[must_use]
    pub fn to_libmir(&self, state_dir: &Path) -> libmir::RuntimeConfig {
        let mut config = libmir::RuntimeConfig::default();
        if let Some(value) = self.kv_block_size {
            config.kv_cache.block_size = value;
        }
        if let Some(value) = self.kv_blocks {
            config.kv_cache.block_count = value;
            config.automatic_kv_cache = false;
        }
        if let Some(value) = self.kv_cache_dtype {
            config.kv_cache.dtype = value;
        }
        if let Some(value) = self.max_batch_requests {
            config.scheduler.max_batch_requests = value;
        }
        if let Some(value) = self.max_batch_tokens {
            config.scheduler.max_batch_tokens = value;
        }
        if let Some(value) = self.prefill_batch_wait_us {
            config.scheduler.prefill_batch_wait_us = value;
        }
        if let Some(value) = self.decode_batch_wait_us {
            config.scheduler.decode_batch_wait_us = value;
        }
        if let Some(value) = self.decode_priority_burst {
            config.scheduler.decode_priority_burst = value;
        }
        config.memory.reserve_percent = self.memory_reserve_percent;
        config.memory.reserve_bytes = self.memory_reserve_bytes;
        if let Some(value) = self.vision_max_pixels {
            config.vision.max_pixels = Some(value);
        }
        if let Some(value) = self.vision_attention_budget_bytes {
            config.vision.attention_budget_bytes = Some(value);
        }
        if let Some(value) = self.vision_memory_percent {
            config.vision.memory_percent = value;
        }
        configure_tuning_cache(&mut config, state_dir);
        config
    }
}

#[cfg(target_os = "macos")]
fn configure_tuning_cache(config: &mut libmir::RuntimeConfig, state_dir: &Path) {
    config.metal.tuning.cache_directory = Some(state_dir.join("tuning/metal"));
}

#[cfg(target_os = "linux")]
fn configure_tuning_cache(config: &mut libmir::RuntimeConfig, state_dir: &Path) {
    config.cuda.tuning.cache_directory = Some(state_dir.join("tuning/cuda"));
    if let Some(include_paths) = cuda_include_paths(std::env::var_os("MIRMIR_CUDA_INCLUDE_PATH")) {
        config.cuda.nvrtc_include_paths = include_paths;
    }
}

#[cfg(target_os = "linux")]
fn cuda_include_paths(value: Option<OsString>) -> Option<Vec<PathBuf>> {
    let paths = value.map(|value| std::env::split_paths(&value).collect::<Vec<_>>())?;
    (!paths.is_empty()).then_some(paths)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn configure_tuning_cache(_config: &mut libmir::RuntimeConfig, _state_dir: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_vision_resource_policy_to_libmir() {
        let settings = RuntimeSettings {
            prefill_batch_wait_us: Some(3_000),
            decode_batch_wait_us: Some(5_000),
            decode_priority_burst: Some(32),
            memory_reserve_percent: Some(1),
            memory_reserve_bytes: Some(512 * 1024 * 1024),
            vision_max_pixels: Some(1_048_576),
            vision_attention_budget_bytes: Some(1_073_741_824),
            vision_memory_percent: Some(25),
            ..RuntimeSettings::default()
        };
        let config = settings.to_libmir(std::path::Path::new("/tmp/mirmir-test"));

        assert_eq!(config.vision.max_pixels, Some(1_048_576));
        assert_eq!(config.vision.attention_budget_bytes, Some(1_073_741_824));
        assert_eq!(config.vision.memory_percent, 25);
        assert_eq!(config.scheduler.prefill_batch_wait_us, 3_000);
        assert_eq!(config.scheduler.decode_batch_wait_us, 5_000);
        assert_eq!(config.scheduler.decode_priority_burst, 32);
        assert_eq!(config.memory.reserve_percent, Some(1));
        assert_eq!(config.memory.reserve_bytes, Some(512 * 1024 * 1024));
    }

    #[test]
    fn explicit_kv_blocks_disable_automatic_sizing() {
        let automatic =
            RuntimeSettings::default().to_libmir(std::path::Path::new("/tmp/mirmir-test"));
        let explicit = RuntimeSettings {
            kv_blocks: Some(1234),
            ..RuntimeSettings::default()
        }
        .to_libmir(std::path::Path::new("/tmp/mirmir-test"));

        assert!(automatic.automatic_kv_cache);
        assert!(!explicit.automatic_kv_cache);
        assert_eq!(explicit.kv_cache.block_count, 1234);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn derives_nvrtc_headers_from_configured_paths() {
        let value = std::env::join_paths([
            "/nix/store/cudart/include",
            "/nix/store/nvcc/include",
            "/nix/store/cccl/include",
        ])
        .expect("static paths are valid");
        let paths = cuda_include_paths(Some(value));

        assert_eq!(
            paths,
            Some(vec![
                PathBuf::from("/nix/store/cudart/include"),
                PathBuf::from("/nix/store/nvcc/include"),
                PathBuf::from("/nix/store/cccl/include"),
            ])
        );
    }

    #[test]
    fn stores_tuning_profiles_below_the_application_state_directory() {
        let state = std::path::Path::new("/tmp/mirmir-state");
        let config = RuntimeSettings::default().to_libmir(state);

        #[cfg(target_os = "macos")]
        assert_eq!(
            config.metal.tuning.cache_directory.as_deref(),
            Some(state.join("tuning/metal").as_path())
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            config.cuda.tuning.cache_directory.as_deref(),
            Some(state.join("tuning/cuda").as_path())
        );
    }
}
