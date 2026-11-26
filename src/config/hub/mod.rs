//! A Hugging Face model reference: a repository, and for a repository of
//! GGUF files the variant (one quantization) to use, written
//! `owner/name:VARIANT` as llama.cpp's `-hf` option takes it.

use std::{fmt, str::FromStr};

use crate::error::{Error, Result};

/// Joins a repository's key to its variant's.
const VARIANT_KEY: char = '@';

#[cfg(test)]
mod tests;

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

    /// Whether `other` names this model: a variant is found by its label in
    /// any case, as the repository's listing matches it.
    #[must_use]
    pub fn is_same_model(&self, other: &Self) -> bool {
        self.repository == other.repository
            && match (&self.variant, &other.variant) {
                (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
                (left, right) => left == right,
            }
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
