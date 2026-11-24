//! A Hub repository of GGUF files as the variants a user picks from: one
//! quantization each, its shards together, beside the `mmproj` vision tower
//! the variants share. Multi-token prediction modules (`mtp-*`) are separate
//! models and are left out.

#[cfg(test)]
mod tests;

/// One file of a repository listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFile {
    pub path: String,
    pub size: u64,
}

/// One quantization of the model: its label and shards, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufVariant {
    pub name: String,
    pub shards: Vec<RepoFile>,
}

impl GgufVariant {
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.shards.iter().map(|shard| shard.size).sum()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GgufRepository {
    /// Smallest first.
    pub variants: Vec<GgufVariant>,
    /// The vision tower: F16 when offered, then BF16, then F32.
    pub vision: Option<RepoFile>,
}

impl GgufRepository {
    /// Groups a listing's GGUF files; empty when it holds none.
    #[must_use]
    pub fn from_files(files: &[RepoFile]) -> Self {
        let gguf = files.iter().filter(|file| file.path.to_ascii_lowercase().ends_with(".gguf"));
        let (towers, text): (Vec<_>, Vec<_>) = gguf
            .filter(|file| !basename(&file.path).to_ascii_lowercase().starts_with("mtp"))
            .partition(|file| basename(&file.path).to_ascii_lowercase().starts_with("mmproj"));
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
        variants.sort_by_key(GgufVariant::bytes);
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

/// The path without `.gguf` and gguf-split's `-00001-of-00003` suffix.
fn unsharded(path: &str) -> String {
    let stem = &path[..path.len() - ".gguf".len()];
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
/// GGML type (`Q4_K_M`, `UD-IQ2_M`, `BF16`), else the whole stem.
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
    stem.to_owned()
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
