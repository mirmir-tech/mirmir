//! Which files of a repository listing are a model's own `SafeTensors`
//! weights: the one test search, its GGUF variants, pull and the header
//! preflight share.

use std::path::Path;

use libmir::{WeightContainer, gguf::is_projector_name};

/// The name a PEFT (`adapter_model`) or MLX-LM (`adapters`) adapter's
/// weights start with.
const ADAPTER_PREFIX: &str = "adapter";

/// Whether `path`, a file of a Hub listing, holds base-model `SafeTensors`
/// weights a downloaded snapshot loads: a `.safetensors` file at the
/// repository's root, where libmir reads weights from, whole
/// (`model.safetensors`, `consolidated.safetensors`) or one shard
/// (`model-00001-of-00002.safetensors`). A file in a subdirectory
/// (`adapters/`, `original/`), an adapter, or a vision projector (`mmproj`)
/// is not a model to download, nor is the shards' index without them.
#[must_use]
pub fn is_base_safetensors(path: &str) -> bool {
    !path.contains('/')
        && WeightContainer::of(Path::new(path)) == Some(WeightContainer::SafeTensors)
        && !path
            .get(..ADAPTER_PREFIX.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(ADAPTER_PREFIX))
        && !is_projector_name(path)
}
