//! How well a catalog model fits this machine, and where it ranks.

use libmir::AdmissionStatus;

use super::CatalogModel;

/// Estimated memory use against the current budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryFit {
    /// Weights and runtime reserve fit with at least 15% headroom.
    Fits,
    /// They fit with less headroom.
    Tight,
    /// They exceed the budget.
    DoesNotFit,
    /// The weights or the budget are unknown.
    Unknown,
}

impl MemoryFit {
    #[must_use]
    pub const fn of(required: Option<u64>, budget: Option<u64>) -> Self {
        match (required, budget) {
            (Some(required), Some(budget)) if required <= budget.saturating_mul(85) / 100 => {
                Self::Fits
            },
            (Some(required), Some(budget)) if required <= budget => Self::Tight,
            (Some(_), Some(_)) => Self::DoesNotFit,
            _ => Self::Unknown,
        }
    }

    /// The protocol spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fits => "fits",
            Self::Tight => "tight",
            Self::DoesNotFit => "does_not_fit",
            Self::Unknown => "unknown",
        }
    }
}

/// How much evidence backs a catalog estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Hub or device metadata is missing.
    Low,
    /// Estimated from Hub metadata and the device budget.
    Medium,
    /// Remote headers were admitted for at least one backend.
    High,
}

impl Confidence {
    /// The protocol spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// The catalog order: admitted models first, then the best memory fit.
pub(super) const fn rank(model: &CatalogModel) -> u8 {
    let compatibility = match model.compatibility {
        AdmissionStatus::Supported => 0,
        AdmissionStatus::Partial => 1,
        AdmissionStatus::Unknown => 2,
        AdmissionStatus::Unsupported => 3,
    };
    let memory = match model.memory_fit {
        MemoryFit::Fits => 0,
        MemoryFit::Tight => 1,
        MemoryFit::Unknown => 2,
        MemoryFit::DoesNotFit => 3,
    };
    compatibility * 4 + memory
}

pub(in crate::catalog) fn reason(compatibility: AdmissionStatus, memory_fit: MemoryFit) -> String {
    let reason = match (compatibility, memory_fit) {
        (AdmissionStatus::Unsupported, _) => {
            "model contract or weight format is not supported by libmir"
        },
        (AdmissionStatus::Unknown, MemoryFit::Fits) => {
            "estimated weights fit; download is required to inspect the execution contract"
        },
        (AdmissionStatus::Unknown, MemoryFit::Tight) => {
            "estimated usage is tight; download is required to inspect the execution contract"
        },
        (AdmissionStatus::Unknown, MemoryFit::Unknown) => {
            "download is required to inspect the model configuration and tensors"
        },
        (_, MemoryFit::DoesNotFit) => "estimated usage exceeds the current memory budget",
        (_, MemoryFit::Fits) => "estimated weights and runtime reserve fit the current budget",
        (_, MemoryFit::Tight) => "estimated usage leaves less than 15% headroom",
        (_, MemoryFit::Unknown) => "insufficient Hub or device metadata for a reliable estimate",
    };
    reason.to_owned()
}
