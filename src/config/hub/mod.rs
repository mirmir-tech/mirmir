//! A Hugging Face model reference: a repository, and for a repository of
//! GGUF files the variant (one quantization) to use, written
//! `owner/name:VARIANT` as llama.cpp's `-hf` option takes it.

use std::{fmt, str::FromStr};

use crate::error::{Error, Result};

/// Joins a repository's key to its variant's.
const VARIANT_KEY: char = '@';

mod scope;
#[cfg(test)]
mod tests;

pub use scope::{CacheKind, HeldCache, RemovalScope, RepositoryCache};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HubReference {
    repository: String,
    variant: Option<String>,
}

impl HubReference {
    /// The repository, `owner/name` or `name`.
    #[must_use]
    pub fn repository(&self) -> &str {
        &self.repository
    }

    /// The GGUF variant, when the reference names one.
    #[must_use]
    pub fn variant(&self) -> Option<&str> {
        self.variant.as_deref()
    }

    /// The same repository with `variant`.
    pub fn with_variant(&self, variant: &str) -> Result<Self> {
        Ok(Self {
            repository: self.repository.clone(),
            variant: Some(valid_variant(variant)?),
        })
    }

    /// Whether `other` names this repository: the Hub resolves an owner and
    /// a name in any case, as a case-insensitive file system does their key.
    #[must_use]
    pub fn same_repository(&self, other: &Self) -> bool {
        self.repository.eq_ignore_ascii_case(&other.repository)
    }

    /// Whether `other` names this model: its repository, and a variant by
    /// its label, in any case, as the repository's listing matches it.
    #[must_use]
    pub fn is_same_model(&self, other: &Self) -> bool {
        self.same_repository(other)
            && match (&self.variant, &other.variant) {
                (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
                (left, right) => left == right,
            }
    }

    /// Whether a file keyed `stem`, a configuration or a download marker,
    /// may hold a model of this repository in any spelling: its key or a
    /// variant's (`@`).
    #[must_use]
    pub fn may_key(&self, stem: &str) -> bool {
        let key = self.repository.replace('/', "--").to_ascii_lowercase();
        let stem = stem.to_ascii_lowercase();
        stem.strip_prefix(&key)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('@'))
    }

    /// Whether a file keyed `stem` is this model's own in any spelling: a
    /// variant's key, or a repository's key and any of its variants'.
    #[must_use]
    pub fn owns_key(&self, stem: &str) -> bool {
        let (key, stem) = (self.key().to_ascii_lowercase(), stem.to_ascii_lowercase());
        stem == key || (self.variant.is_none() && stem.starts_with(&format!("{key}@")))
    }

    /// The name of the repository's directory in a Hugging Face cache:
    /// `models--owner--name`, whichever variant the reference names.
    #[must_use]
    pub fn cache_directory(&self) -> String {
        format!("models--{}", self.repository.replace('/', "--"))
    }

    /// Whether `label` can name a variant: `owner/name:LABEL`.
    #[must_use]
    pub fn is_variant_label(label: &str) -> bool {
        valid_variant(label).is_ok()
    }

    /// The file-system key of the model: `owner--name`, then `@VARIANT`.
    /// No repository or variant part holds `@`, so a variant of `name`
    /// (`name@VARIANT`) never meets the repository `name/VARIANT`
    /// (`name--VARIANT`), and a repository's key is the one it always had.
    #[must_use]
    pub fn key(&self) -> String {
        let repository = self.repository.replace('/', "--");
        match &self.variant {
            Some(variant) => format!("{repository}{VARIANT_KEY}{variant}"),
            None => repository,
        }
    }
}

impl FromStr for HubReference {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let (repository, variant) = match value.split_once(':') {
            Some((repository, variant)) => (repository, Some(valid_variant(variant)?)),
            None => (value, None),
        };
        let parts = repository.split('/').collect::<Vec<_>>();
        if !(1..=2).contains(&parts.len()) || !parts.iter().all(|part| valid_part(part)) {
            return Err(Error::Config(format!("invalid Hugging Face repository id `{value}`")));
        }
        Ok(Self {
            repository: repository.to_owned(),
            variant,
        })
    }
}

impl fmt::Display for HubReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.variant {
            Some(variant) => write!(formatter, "{}:{variant}", self.repository),
            None => formatter.write_str(&self.repository),
        }
    }
}

fn valid_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
}

fn valid_variant(variant: &str) -> Result<String> {
    if valid_part(variant) && !variant.contains("--") {
        Ok(variant.to_owned())
    } else {
        Err(Error::Config(format!("invalid GGUF variant `{variant}`")))
    }
}
