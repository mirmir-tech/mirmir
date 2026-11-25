//! A Hub repository of GGUF files as the variants a user picks from: one
//! quantization each, its shards together, beside the `mmproj` vision tower
//! the variants share. Multi-token prediction modules (`mtp-*`) are separate
//! models and are left out.

mod quantization;
#[cfg(test)]
mod tests;

use std::path::Path;

use libmir::{
    WeightContainer,
    gguf::{ShardName, is_projector_name},
};
pub use quantization::{Q4_K_M, Q8_0, Quantization};

/// The prefix of the multi-token prediction modules a repository may ship.
const PREDICTION_MODULE_PREFIX: &str = "mtp";
/// The weights of a typical vision tower with its projector, between 400 and
/// 670 million, to estimate its size before its file is listed.
const VISION_PARAMETERS: u64 = 600_000_000;
/// The bits per weight of a vision tower whose name gives none: F16.
const VISION_BITS: f64 = 16.0;

/// One file of a repository listing; Hub search lists files without sizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFile {
    pub path: String,
    pub size: Option<u64>,
}

/// One quantization of the model: its name, its label unless labels clash,
/// and its shards, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufVariant {
    pub name: String,
    pub quantization: Quantization,
    pub shards: Vec<RepoFile>,
}

impl GgufVariant {
    /// The shards' bytes, when the listing gave every size.
    #[must_use]
    pub fn bytes(&self) -> Option<u64> {
        self.shards.iter().map(|shard| shard.size).sum()
    }

    /// The bits per weight its quantization names.
    #[must_use]
    pub fn bits_per_weight(&self) -> Option<f64> {
        self.quantization.bits_per_weight()
    }

    /// Its size estimated for a model of `parameters` weights.
    #[must_use]
    pub fn estimated_bytes(&self, parameters: u64) -> Option<u64> {
        self.bits_per_weight().map(|bits| quantization::bytes(parameters, bits))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GgufRepository {
    /// Fewest bits per weight first, then unknown types by name; the same
    /// order whether the files came with sizes or not.
    pub variants: Vec<GgufVariant>,
    /// The vision tower: F16 when offered, then BF16, then F32.
    pub vision: Option<RepoFile>,
    /// Whether the listing also holds `SafeTensors` weights, which a pull
    /// without a variant downloads as one model.
    safetensors: bool,
}

impl GgufRepository {
    /// Groups a listing's GGUF files; empty when it holds none.
    #[must_use]
    pub fn from_files(files: &[RepoFile]) -> Self {
        let container = |file: &&RepoFile| WeightContainer::of(Path::new(&file.path));
        let safetensors =
            files.iter().any(|file| container(&file) == Some(WeightContainer::SafeTensors));
        let gguf = files.iter().filter(|file| container(file) == Some(WeightContainer::Gguf));
        let (towers, text): (Vec<_>, Vec<_>) = gguf
            .filter(|file| !is_prediction_module(basename(&file.path)))
            .partition(|file| is_projector_name(basename(&file.path)));
        let mut groups: Vec<(String, Vec<RepoFile>)> = Vec::new();
        for file in text {
            let stem = unsharded(&file.path);
            match groups.iter_mut().find(|(group, _)| *group == stem) {
                Some((_, shards)) => shards.push(file.clone()),
                None => groups.push((stem, vec![file.clone()])),
            }
        }
        let kinds = groups
            .iter()
            .map(|(stem, _)| Quantization::of_stem(basename(stem)))
            .collect::<Vec<_>>();
        let unique = kinds
            .iter()
            .enumerate()
            .all(|(index, kind)| kinds.iter().skip(index + 1).all(|other| !other.is(kind.label())));
        let mut variants = groups
            .into_iter()
            .zip(kinds)
            .map(|((stem, mut shards), quantization)| {
                shards.sort_by(|left, right| left.path.cmp(&right.path));
                let name = if unique {
                    quantization.label().to_owned()
                } else {
                    stem.replace('/', "_")
                };
                GgufVariant { name, quantization, shards }
            })
            .collect::<Vec<_>>();
        variants.sort_by(|left, right| {
            let bits = |variant: &GgufVariant| variant.bits_per_weight().unwrap_or(f64::INFINITY);
            bits(left).total_cmp(&bits(right)).then_with(|| left.name.cmp(&right.name))
        });
        Self {
            variants,
            vision: vision(towers),
            safetensors,
        }
    }

    /// Whether the listing is GGUF variants only, with no `SafeTensors`
    /// weights to download as one model: the repositories a pull without a
    /// variant takes the default variant of, and the catalog lists the
    /// variants of.
    #[must_use]
    pub const fn gguf_only(&self) -> bool {
        !self.variants.is_empty() && !self.safetensors
    }

    /// The vision tower's size estimated before its file is listed, at the
    /// precision its name gives, else F16; `None` without a tower.
    #[must_use]
    pub fn estimated_vision_bytes(&self) -> Option<u64> {
        let tower = self.vision.as_ref()?;
        let kind = Quantization::of_stem(basename(&unsharded(&tower.path)));
        let bits = kind.bits_per_weight().unwrap_or(VISION_BITS);
        Some(quantization::bytes(VISION_PARAMETERS, bits))
    }

    /// The variant `name` names: its label or its file stem, in any case.
    #[must_use]
    pub fn variant(&self, name: &str) -> Option<&GgufVariant> {
        self.variants.iter().find(|variant| {
            variant.name.eq_ignore_ascii_case(name)
                || variant.shards.first().is_some_and(|shard| {
                    basename(&unsharded(&shard.path)).eq_ignore_ascii_case(name)
                })
        })
    }

    /// The variant to download when none is named, the one rule of search
    /// and pull: `Q8_0` when it `fits`, nearly lossless at half of BF16's
    /// size; else `Q4_K_M`, which llama.cpp's `-hf` takes; else the largest
    /// known type that fits, else a type of unknown size that fits, else the
    /// smallest.
    pub fn default_variant(&self, fits: impl Fn(&GgufVariant) -> bool) -> Option<&GgufVariant> {
        let named = |name: &str| self.variants.iter().find(|variant| variant.quantization.is(name));
        let largest = |known: bool| {
            self.variants
                .iter()
                .rev()
                .filter(|variant| variant.bits_per_weight().is_some() == known)
                .find(|variant| fits(variant))
        };
        named(Q8_0)
            .filter(|variant| fits(variant))
            .or_else(|| named(Q4_K_M))
            .or_else(|| largest(true))
            .or_else(|| largest(false))
            .or_else(|| self.variants.first())
    }

    /// The variants' names, smallest first, for messages.
    #[must_use]
    pub fn names(&self) -> String {
        self.variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Whether a file is a multi-token prediction module (`mtp-*`), a model of
/// its own.
fn is_prediction_module(name: &str) -> bool {
    name.get(..PREDICTION_MODULE_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(PREDICTION_MODULE_PREFIX))
}

/// The path of a GGUF file without its extension and gguf-split's
/// `-00001-of-00003` suffix.
fn unsharded(path: &str) -> String {
    let stem = path.rsplit_once('.').map_or(path, |(stem, _)| stem);
    ShardName::parse(stem).map_or(stem, |shard| shard.prefix).to_owned()
}

fn vision(towers: Vec<&RepoFile>) -> Option<RepoFile> {
    let precision = |file: &RepoFile| {
        let name = basename(&file.path).to_ascii_lowercase();
        if name.contains("bf16") {
            1
        } else if name.contains("f16") {
            0
        } else if name.contains("f32") {
            2
        } else {
            3
        }
    };
    towers
        .into_iter()
        .min_by_key(|file| (precision(file), file.path.clone()))
        .cloned()
}
