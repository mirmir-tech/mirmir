//! Which files of a repository listing are a model's own `SafeTensors`
//! weights: the one test search, its GGUF variants, pull and the header
//! preflight share.

use std::path::Path;

use libmir::{WeightContainer, gguf::is_projector_name};

/// The name a PEFT (`adapter_model`) or MLX-LM (`adapters`) adapter's
/// weights start with.
const ADAPTER_PREFIX: &str = "adapter";
/// The root index file of a sharded `SafeTensors` checkpoint.
const SAFETENSORS_INDEX: &str = "model.safetensors.index.json";

/// Whether `name` (a file basename) names an adapter or a vision projector
/// rather than base-model weights.
fn is_auxiliary_safetensors_name(name: &str) -> bool {
    name.get(..ADAPTER_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(ADAPTER_PREFIX))
        || is_projector_name(name)
}

/// Whether `path`, a file of a Hub listing, holds unindexed root base-model
/// `SafeTensors` weights: whole (`model.safetensors`,
/// `consolidated.safetensors`) or a root shard
/// (`model-00001-of-00002.safetensors`). A file in a subdirectory (`adapters/`,
/// `original/`), an adapter, or a vision projector (`mmproj`) is not an
/// unindexed base model, nor is the shards' index alone.
#[must_use]
pub fn is_base_safetensors(path: &str) -> bool {
    !path.contains('/')
        && WeightContainer::of(Path::new(path)) == Some(WeightContainer::SafeTensors)
        && !is_auxiliary_safetensors_name(path)
}

/// Whether `path` can be a shard referenced by a root
/// `model.safetensors.index.json`: any `.safetensors` file whose basename is
/// neither an adapter nor a projector, at the root or in a subdirectory
/// (such as `weights/model-00001-of-00002.safetensors`).
fn is_indexable_safetensors_shard(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    WeightContainer::of(Path::new(name)) == Some(WeightContainer::SafeTensors)
        && !is_auxiliary_safetensors_name(name)
}

/// Whether a repository listing holds base-model `SafeTensors` weights:
/// either an unindexed root `.safetensors` file, or a root
/// `model.safetensors.index.json` alongside at least one non-auxiliary
/// `.safetensors` shard (including in a subdirectory such as `weights/`).
#[must_use]
pub fn has_base_safetensors<'a>(paths: impl IntoIterator<Item = &'a str>) -> bool {
    let mut has_index = false;
    let mut has_shard = false;
    for path in paths {
        if is_base_safetensors(path) {
            return true;
        }
        if path == SAFETENSORS_INDEX {
            has_index = true;
        } else if is_indexable_safetensors_shard(path) {
            has_shard = true;
        }
        if has_index && has_shard {
            return true;
        }
    }
    false
}
