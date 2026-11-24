//! A Hub repository of GGUF files as the variants a user picks from: one
//! quantization each, its shards together, beside the `mmproj` vision tower
//! the variants share. Multi-token prediction modules (`mtp-*`) are separate
//! models and are left out.

mod quantization;
#[cfg(test)]
mod tests;

use std::path::Path;

use libmir::{WeightContainer, gguf::is_projector_name};

/// The prefix of the multi-token prediction modules a repository may ship.
const PREDICTION_MODULE_PREFIX: &str = "mtp";

/// One file of a repository listing; Hub search lists files without sizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFile {
    pub path: String,
    pub size: Option<u64>,
}

/// One quantization of the model: its label and shards, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufVariant {
    pub name: String,
    pub shards: Vec<RepoFile>,
}

impl GgufVariant {
    /// The shards' bytes, when the listing gave every size.
    #[must_use]
    pub fn bytes(&self) -> Option<u64> {
        self.shards.iter().map(|shard| shard.size).sum()
    }

    /// The bits per weight its label names.
    #[must_use]
    pub fn bits_per_weight(&self) -> Option<f64> {
        quantization::bits_per_weight(&self.name)
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
}

impl GgufRepository {
    /// Groups a listing's GGUF files; empty when it holds none.
    #[must_use]
    pub fn from_files(files: &[RepoFile]) -> Self {
        let gguf = files.iter().filter(|file| {
            WeightContainer::of(Path::new(&file.path)) == Some(WeightContainer::Gguf)
        });
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
        let labels = groups.iter().map(|(stem, _)| label(basename(stem))).collect::<Vec<_>>();
        let unique = labels.iter().enumerate().all(|(index, name)| {
            labels.iter().skip(index + 1).all(|other| !other.eq_ignore_ascii_case(name))
        });
        let mut variants = groups
            .into_iter()
            .zip(labels)
            .map(|((stem, mut shards), label)| {
                shards.sort_by(|left, right| left.path.cmp(&right.path));
                let name = if unique {
                    label
                } else {
                    stem.replace('/', "_")
                };
                GgufVariant { name, shards }
            })
            .collect::<Vec<_>>();
        variants.sort_by(|left, right| {
            let bits = |variant: &GgufVariant| variant.bits_per_weight().unwrap_or(f64::INFINITY);
            bits(left).total_cmp(&bits(right)).then_with(|| left.name.cmp(&right.name))
        });
        Self { variants, vision: vision(towers) }
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

    /// The variant to download when none is named, as llama.cpp's `-hf`
    /// takes `Q4_K_M`; without it `Q8_0` when it `fits`, then the largest that
    /// fits, else the smallest.
    pub fn default_variant(&self, fits: impl Fn(&GgufVariant) -> bool) -> Option<&GgufVariant> {
        let named = |name: &str| {
            self.variants.iter().find(|variant| variant.name.eq_ignore_ascii_case(name))
        };
        named("Q4_K_M")
            .or_else(|| named("Q8_0").filter(|variant| fits(variant)))
            .or_else(|| self.variants.iter().rev().find(|variant| fits(variant)))
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
    let bytes = stem.as_bytes();
    let shard = bytes.len() >= 15
        && bytes[bytes.len() - 15] == b'-'
        && stem[stem.len() - 9..stem.len() - 5].eq_ignore_ascii_case("-of-")
        && bytes[bytes.len() - 14..bytes.len() - 9].iter().all(u8::is_ascii_digit)
        && bytes[bytes.len() - 5..].iter().all(u8::is_ascii_digit);
    if shard {
        stem[..stem.len() - 15].to_owned()
    } else {
        stem.to_owned()
    }
}

/// The quantization label of a file stem: from its first token naming a
/// GGML type (`Q4_K_M`, `UD-IQ2_M`, `BF16`), else a type glued to a token by
/// `_`, else the whole stem.
fn label(stem: &str) -> String {
    let tokens = stem.split(['-', '.']).collect::<Vec<_>>();
    let mut offset = 0;
    for (index, token) in tokens.iter().enumerate() {
        let next = tokens.get(index + 1).copied().unwrap_or_default();
        if quantization(token) || token.eq_ignore_ascii_case("ud") && quantization(next) {
            return stem[offset..].to_owned();
        }
        offset += token.len() + 1;
    }
    // A type glued to the name by `_` (`gemma-4-E4B_q4_0-it`) is the type.
    tokens
        .iter()
        .find_map(|token| {
            let (index, _) =
                token.match_indices('_').find(|&(index, _)| quantization(&token[index + 1..]))?;
            Some(token[index + 1..].to_owned())
        })
        .unwrap_or_else(|| stem.to_owned())
}

fn quantization(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    let numbered = |prefix: &str| {
        lower
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
    };
    numbered("q")
        || numbered("iq")
        || numbered("tq")
        || ["bf16", "f16", "f32", "mxfp4", "nvfp4"]
            .iter()
            .any(|kind| lower.starts_with(kind))
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
