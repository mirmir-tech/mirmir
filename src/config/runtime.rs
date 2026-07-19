use super::schema::RuntimeSettings;

impl RuntimeSettings {
    #[must_use]
    pub fn to_libmir(&self) -> libmir::RuntimeConfig {
        let mut config = libmir::RuntimeConfig::default();
        if let Some(value) = self.kv_block_size {
            config.kv_cache.block_size = value;
        }
        if let Some(value) = self.kv_blocks {
            config.kv_cache.block_count = value;
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
        if let Some(value) = self.vision_max_pixels {
            config.vision.max_pixels = Some(value);
        }
        if let Some(value) = self.vision_attention_budget_bytes {
            config.vision.attention_budget_bytes = Some(value);
        }
        if let Some(value) = self.vision_memory_percent {
            config.vision.memory_percent = value;
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_vision_resource_policy_to_libmir() {
        let settings = RuntimeSettings {
            vision_max_pixels: Some(1_048_576),
            vision_attention_budget_bytes: Some(1_073_741_824),
            vision_memory_percent: Some(25),
            ..RuntimeSettings::default()
        };
        let config = settings.to_libmir();

        assert_eq!(config.vision.max_pixels, Some(1_048_576));
        assert_eq!(config.vision.attention_budget_bytes, Some(1_073_741_824));
        assert_eq!(config.vision.memory_percent, 25);
    }
}
