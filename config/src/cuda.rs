use std::path::PathBuf;

use clap::{Args, ValueEnum};
use libmir::cuda::{
    CudaConfig, CudaKernelAdmission, CudaMoeBatchPolicy, CudaNumericalPolicy, CudaOutputHeadPolicy,
    CudaTuningMode,
};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum TuningArg {
    Disabled,
    Cached,
    Startup,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum MoeBatchArg {
    Auto,
    W4a4,
    W4a4Direct,
    W4a4Hybrid,
    W4a4Bucketed,
    W4a16,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputHeadArg {
    Auto,
    Bf16,
    Fp8Blockwise,
    Fp8Vectorized,
    Fp8Residual,
    Fp8BlockVectorized,
    Fp8BlockRefined,
}

#[derive(Debug, Args, Default)]
pub struct CudaArgs {
    #[arg(long, env = "MIRMIR_CUDA_DEVICE")]
    device: Option<usize>,
    #[arg(long, env = "MIRMIR_CUDA_PREFILL_CHUNK_TOKENS")]
    prefill_chunk_tokens: Option<usize>,
    #[arg(long, env = "MIRMIR_CUDA_MEMORY_POOL_RETAIN_BYTES")]
    memory_pool_retain_bytes: Option<u64>,
    #[arg(long, env = "MIRMIR_CUDA_INCLUDE_PATH", value_delimiter = ':')]
    include_paths: Vec<PathBuf>,
    #[arg(long, env = "MIRMIR_CUDA_KERNEL_CACHE")]
    kernel_cache: Option<PathBuf>,
    #[arg(long, env = "MIRMIR_CUDA_TUNING", value_enum)]
    tuning: Option<TuningArg>,
    #[arg(long, env = "MIRMIR_CUDA_TUNING_CACHE")]
    tuning_cache: Option<PathBuf>,
    #[arg(long, env = "MIRMIR_CUDA_TUNING_BUDGET_MS")]
    tuning_budget_ms: Option<u64>,
    #[arg(long, env = "MIRMIR_CUDA_MOE_BATCH", value_enum)]
    moe_batch: Option<MoeBatchArg>,
    #[arg(long, env = "MIRMIR_CUDA_OUTPUT_HEAD", value_enum)]
    output_head: Option<OutputHeadArg>,
}

impl CudaArgs {
    pub(super) fn apply_to(&self, config: &mut CudaConfig) {
        if let Some(device) = self.device {
            config.device_ordinal = device;
        }
        if let Some(tokens) = self.prefill_chunk_tokens {
            config.model_session.prefill_chunk_tokens = tokens;
        }
        if let Some(bytes) = self.memory_pool_retain_bytes {
            config.memory_pool_release_threshold = bytes;
        }
        if !self.include_paths.is_empty() {
            config.nvrtc_include_paths.clone_from(&self.include_paths);
        }
        config.nvrtc_cache_directory = self.kernel_cache.clone().or_else(default_kernel_cache);
        config.tuning.cache_directory = self.tuning_cache.clone().or_else(default_tuning_cache);
        if let Some(mode) = self.tuning {
            config.tuning.mode = mode.into();
        }
        if let Some(budget) = self.tuning_budget_ms {
            config.tuning.startup_budget_ms = budget;
        }
        if let Some(policy) = self.moe_batch {
            config.planning.moe_batch = policy.into();
        }
        if let Some(policy) = self.output_head {
            config.planning.output_head = policy.into();
            if !matches!(policy, OutputHeadArg::Auto | OutputHeadArg::Bf16) {
                config.planning.numerical = CudaNumericalPolicy::Throughput;
                config.planning.admission = CudaKernelAdmission::Experimental;
            }
        }
    }
}

impl From<TuningArg> for CudaTuningMode {
    fn from(value: TuningArg) -> Self {
        match value {
            TuningArg::Disabled => Self::Disabled,
            TuningArg::Cached => Self::Cached,
            TuningArg::Startup => Self::Startup,
        }
    }
}

impl From<OutputHeadArg> for CudaOutputHeadPolicy {
    fn from(value: OutputHeadArg) -> Self {
        match value {
            OutputHeadArg::Auto => Self::Auto,
            OutputHeadArg::Bf16 => Self::Bf16,
            OutputHeadArg::Fp8Blockwise => Self::Fp8Blockwise,
            OutputHeadArg::Fp8Vectorized => Self::Fp8Vectorized,
            OutputHeadArg::Fp8Residual => Self::Fp8Residual,
            OutputHeadArg::Fp8BlockVectorized => Self::Fp8BlockVectorized,
            OutputHeadArg::Fp8BlockRefined => Self::Fp8BlockRefined,
        }
    }
}

impl From<MoeBatchArg> for CudaMoeBatchPolicy {
    fn from(value: MoeBatchArg) -> Self {
        match value {
            MoeBatchArg::Auto => Self::Auto,
            MoeBatchArg::W4a4 => Self::W4A4,
            MoeBatchArg::W4a4Direct => Self::W4A4Direct,
            MoeBatchArg::W4a4Hybrid => Self::W4A4Hybrid,
            MoeBatchArg::W4a4Bucketed => Self::W4A4Bucketed,
            MoeBatchArg::W4a16 => Self::W4A16,
        }
    }
}

fn default_kernel_cache() -> Option<PathBuf> {
    default_cache_root().map(|root| root.join("mirmir/cuda/ptx-v1"))
}

fn default_tuning_cache() -> Option<PathBuf> {
    default_cache_root().map(|root| root.join("mirmir/cuda/tuning-v1"))
}

fn default_cache_root() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
}
