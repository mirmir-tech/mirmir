//! A GGUF repository's variants as the choices of one catalog row: each
//! with its size, exact once its files are listed and estimated before from
//! the model's parameter count, its memory fit and whether it is downloaded.

use libmir::AdmissionStatus;

use super::{MachineMemory, MemoryFit, required};
use crate::catalog::hub::{GgufRepository, GgufVariant};

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

    #[must_use]
    pub const fn bytes(self) -> Option<u64> {
        match self {
            Self::Exact(bytes) | Self::Estimated(bytes) => Some(bytes),
            Self::Unknown => None,
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
/// when none is chosen; sizes are estimated from `parameters` where the
/// listing gave none.
#[must_use]
pub fn variants(
    repository: &GgufRepository,
    parameters: Option<u64>,
    memory: MachineMemory,
) -> (Vec<CatalogVariant>, Option<String>) {
    let vision = repository.vision.as_ref().and_then(|tower| tower.size).unwrap_or(0);
    let size = |variant: &GgufVariant| match (variant.bytes(), parameters) {
        (Some(bytes), _) => VariantSize::Exact(bytes + vision),
        (None, Some(parameters)) => variant
            .estimated_bytes(parameters)
            .map_or(VariantSize::Unknown, VariantSize::Estimated),
        (None, None) => VariantSize::Unknown,
    };
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
