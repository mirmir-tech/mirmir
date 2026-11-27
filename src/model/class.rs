//! A model's class, the name catalogs and model lists show it by.

/// The class of a model nothing names.
const UNKNOWN: &str = "unknown";

/// The configuration's `architectures` joined, else the first non-empty of
/// `fallbacks` (its `model_type`, then a GGUF file's
/// `general.architecture`), else unknown.
#[must_use]
pub fn name<'a>(
    architectures: impl IntoIterator<Item = &'a str>,
    fallbacks: impl IntoIterator<Item = Option<&'a str>>,
) -> String {
    let architectures = architectures
        .into_iter()
        .filter(|architecture| !architecture.is_empty())
        .collect::<Vec<_>>();
    if !architectures.is_empty() {
        return architectures.join(", ");
    }
    fallbacks
        .into_iter()
        .flatten()
        .find(|name| !name.is_empty())
        .unwrap_or(UNKNOWN)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::name;

    #[test]
    fn names_architectures_then_the_first_named_fallback() {
        assert_eq!(name(["A", "B"], [Some("a")]), "A, B");
        assert_eq!(name([""], [Some(""), None, Some("gemma4")]), "gemma4");
        assert_eq!(name([], [None, Some("")]), "unknown");
    }
}
