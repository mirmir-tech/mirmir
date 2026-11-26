//! A variant's name as the label a reference (`owner/name:LABEL`) gives
//! it: a third-party repository may name a file with characters no label
//! takes (`Model-Q4_K_M (imatrix).gguf`, `Model-Q4_K_M--v2.gguf`), which
//! would leave its variant impossible to name, save or remove.

use super::GgufVariant;
use crate::config::HubReference;

/// The `variants`, each named by a label: a name a label takes is kept,
/// as every variant saved before such names were turned had, so its key
/// never changes; another one is turned into a label (`.` and `..` and a
/// name another variant has in another case too), and one that two
/// variants would then share, in any case, takes the first free `_N`
/// suffix, in the order of the variants' first files. No variant is left
/// out.
pub(super) fn labelled(variants: impl Iterator<Item = GgufVariant>) -> Vec<GgufVariant> {
    let (mut kept, mut turned) = (Vec::new(), Vec::new());
    for variant in variants {
        let label = label(&variant.name);
        let own = label == variant.name && HubReference::is_variant_label(&label);
        if own && !kept.iter().any(|other: &GgufVariant| other.name.eq_ignore_ascii_case(&label)) {
            kept.push(variant);
        } else {
            turned.push(GgufVariant { name: label, ..variant });
        }
    }
    turned.sort_by(|left, right| first_file(left).cmp(first_file(right)));
    for mut variant in turned {
        let taken = |name: &str, kept: &[GgufVariant]| {
            !HubReference::is_variant_label(name)
                || kept.iter().any(|other| other.name.eq_ignore_ascii_case(name))
        };
        if taken(&variant.name, &kept) {
            // One of as many suffixes as there are names, and one more, is free.
            let free = (2..=kept.len() + 2)
                .map(|suffix| format!("{}_{suffix}", variant.name))
                .find(|name| !taken(name, &kept));
            variant.name = free.unwrap_or_default();
        }
        kept.push(variant);
    }
    kept
}

fn first_file(variant: &GgufVariant) -> &str {
    variant.shards.first().map_or("", |shard| shard.path.as_str())
}

/// `name` with each character a label does not take turned into `_`, and
/// each run of `-` into one, as `--` joins a legacy variant key.
fn label(name: &str) -> String {
    let mut label = String::with_capacity(name.len());
    for character in name.chars() {
        let character = if character.is_ascii_alphanumeric() || "-_.".contains(character) {
            character
        } else {
            '_'
        };
        if !(character == '-' && label.ends_with('-')) {
            label.push(character);
        }
    }
    label
}
