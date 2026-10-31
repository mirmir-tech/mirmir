use serde::{Deserialize, Serialize};

/// Runtime settings of one model that take the place of `[runtime]`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelRuntimeConfig {
    /// K/V cache blocks for this model alone, for models such as rerankers
    /// whose requests need far less cache than the shared setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kv_blocks: Option<std::num::NonZeroU32>,
}

impl ModelRuntimeConfig {
    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes a reference"
    )]
    pub(super) const fn is_unset(&self) -> bool {
        self.kv_blocks.is_none()
    }

    /// Load options for this model; `force` admits it over the memory budget.
    pub const fn load_options(self, force: bool) -> libmir::ModelLoadOptions {
        libmir::ModelLoadOptions {
            allow_memory_overcommit: force,
            kv_blocks: self.kv_blocks,
        }
    }
}
