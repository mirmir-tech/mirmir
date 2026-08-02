use clap::Args;
use libmir::{KvCacheDType, RuntimeConfig};

#[derive(Debug, Args, Default)]
pub struct RuntimeArgs {
    #[arg(long, env = "MIRMIR_KV_BLOCK_SIZE")]
    pub kv_block_size: Option<usize>,
    #[arg(long, env = "MIRMIR_KV_BLOCKS")]
    pub kv_blocks: Option<u32>,
    #[arg(long, env = "MIRMIR_KV_CACHE_DTYPE")]
    pub kv_cache_dtype: Option<KvCacheDType>,
    #[arg(long, env = "MIRMIR_MAX_BATCH_REQUESTS")]
    pub max_batch_requests: Option<usize>,
    #[arg(long, env = "MIRMIR_MAX_BATCH_TOKENS")]
    pub max_batch_tokens: Option<usize>,
    #[arg(long, env = "MIRMIR_DECODE_BATCH_WAIT_US")]
    pub decode_batch_wait_us: Option<u64>,
    #[arg(long, env = "MIRMIR_DECODE_PRIORITY_BURST")]
    pub decode_priority_burst: Option<usize>,
    #[cfg(target_os = "linux")]
    #[command(flatten)]
    pub cuda: crate::CudaArgs,
    #[cfg(target_os = "macos")]
    #[command(flatten)]
    pub metal: crate::MetalArgs,
}

impl RuntimeArgs {
    #[must_use]
    pub fn runtime_config(&self) -> RuntimeConfig {
        let mut config = RuntimeConfig::default();
        if let Some(block_size) = self.kv_block_size {
            config.kv_cache.block_size = block_size;
        }
        if let Some(blocks) = self.kv_blocks {
            config.kv_cache.block_count = blocks;
            config.automatic_kv_cache = false;
        }
        if let Some(dtype) = self.kv_cache_dtype {
            config.kv_cache.dtype = dtype;
        }
        if let Some(requests) = self.max_batch_requests {
            config.scheduler.max_batch_requests = requests;
        }
        if let Some(tokens) = self.max_batch_tokens {
            config.scheduler.max_batch_tokens = tokens;
        }
        if let Some(wait) = self.decode_batch_wait_us {
            config.scheduler.decode_batch_wait_us = wait;
        }
        if let Some(burst) = self.decode_priority_burst {
            config.scheduler.decode_priority_burst = burst;
        }
        #[cfg(target_os = "linux")]
        self.cuda.apply_to(&mut config.cuda);
        #[cfg(target_os = "macos")]
        self.metal.apply_to(&mut config.metal);
        config
    }
}
