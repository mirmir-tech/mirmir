//! A GGUF repository's variants as the choices of one catalog row: each
//! with its size, exact once its files are listed and estimated before from
//! the model's parameter count, its memory fit and whether it is downloaded.

use libmir::AdmissionStatus;

use super::{CatalogModel, MachineMemory, MemoryFit, required};
use crate::{
    catalog::hub::{GgufRepository, GgufVariant},
    error::Result,
};

/// How much is known of a variant's size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantSize {
    /// The files' listed sizes.
    Exact(u64),
    /// The parameter count at the type's bits per weight.
    Estimated(u64),
    Unknown,
}

impl VariantSize {
    /// The protocol spelling.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Exact(_) => "exact",
            Self::Estimated(_) => "estimated",
            Self::Unknown => "unknown",
        }
    }

    /// The size a protocol spelling of its kind names with `bytes`;
    /// unknown for a kind this build does not know or without bytes.
    #[must_use]
    pub fn from_protocol(kind: &str, bytes: Option<u64>) -> Self {
        bytes
            .into_iter()
            .flat_map(|bytes| [Self::Exact(bytes), Self::Estimated(bytes)])
            .find(|size| size.kind() == kind)
            .unwrap_or(Self::Unknown)
    }

    #[must_use]
    pub const fn bytes(self) -> Option<u64> {
        match self {
            Self::Exact(bytes) | Self::Estimated(bytes) => Some(bytes),
            Self::Unknown => None,
        }
    }

    /// Both sizes together: exact only when both are.
    const fn plus(self, other: Self) -> Self {
        match (self, other) {
            (Self::Exact(left), Self::Exact(right)) => Self::Exact(left.saturating_add(right)),
            (
                Self::Exact(left) | Self::Estimated(left),
                Self::Exact(right) | Self::Estimated(right),
            ) => Self::Estimated(left.saturating_add(right)),
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogVariant {
    pub name: String,
    /// The text model's files with the vision tower, when it is offered.
    pub size: VariantSize,
    pub required_bytes: Option<u64>,
    pub memory_fit: MemoryFit,
    pub downloaded: bool,
}

/// The repository's variants, fewest bits first, and the one to download
/// when none is chosen, by the rule search and pull share; sizes, the
/// vision tower's with each, are estimated from `parameters` where the
/// listing gave none.
#[must_use]
pub fn variants(
    repository: &GgufRepository,
    parameters: Option<u64>,
    memory: MachineMemory,
) -> (Vec<CatalogVariant>, Option<String>) {
    let vision = repository.vision.as_ref().map_or(VariantSize::Exact(0), |tower| {
        tower.size.map_or_else(
            || {
                repository
                    .estimated_vision_bytes()
                    .map_or(VariantSize::Unknown, VariantSize::Estimated)
            },
            VariantSize::Exact,
        )
    });
    let text = |variant: &GgufVariant| match (variant.bytes(), parameters) {
        (Some(bytes), _) => VariantSize::Exact(bytes),
        (None, Some(parameters)) => variant
            .estimated_bytes(parameters)
            .map_or(VariantSize::Unknown, VariantSize::Estimated),
        (None, None) => VariantSize::Unknown,
    };
    let size = |variant: &GgufVariant| text(variant).plus(vision);
    let fit =
        |variant: &GgufVariant| MemoryFit::of(size(variant).bytes().map(required), memory.budget);
    let default = repository
        .default_variant(|variant| fit(variant) == MemoryFit::Fits)
        .map(|variant| variant.name.clone());
    let variants = repository
        .variants
        .iter()
        .map(|variant| CatalogVariant {
            name: variant.name.clone(),
            size: size(variant),
            required_bytes: size(variant).bytes().map(required),
            memory_fit: fit(variant),
            downloaded: false,
        })
        .collect();
    (variants, default)
}

impl CatalogModel {
    /// Whether the row's own pull target is downloaded: a GGUF-only
    /// repository's default variant, by its flag, else the repository, as
    /// `repository` tells; a variant's snapshot shares the repository's
    /// cache, so the cache does not tell for a GGUF-only one.
    pub(in crate::catalog) fn target_downloaded(
        &self,
        repository: impl FnOnce() -> Result<bool>,
    ) -> Result<bool> {
        self.default_variant.as_ref().map_or_else(repository, |name| {
            Ok(self.variants.iter().any(|variant| &variant.name == name && variant.downloaded))
        })
    }
}

/// Whether the repository's GGUF files import, by the architecture the Hub
/// read from them; a backend's admission waits for the variant's header.
#[must_use]
pub fn compatibility(architecture: Option<&str>, repository: &GgufRepository) -> AdmissionStatus {
    match architecture {
        _ if repository.variants.is_empty() => AdmissionStatus::Unsupported,
        Some(architecture) if !libmir::gguf::imports_architecture(architecture) => {
            AdmissionStatus::Unsupported
        },
        Some(_) | None => AdmissionStatus::Unknown,
    }
}
