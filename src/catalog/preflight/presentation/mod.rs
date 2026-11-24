use libmir::{AdmissionStatus, BackendTarget, RemoteModelContract};

use super::RemoteHeaderPreflight;
use crate::catalog::{CatalogModel, Confidence};

pub(in crate::catalog) fn apply(model: &mut CatalogModel, preflight: &RemoteHeaderPreflight) {
    model.preflight_bytes = Some(preflight.fetched_bytes);
    model.preflight_error.clone_from(&preflight.contract_error);
    let Some(contract) = preflight.contract.as_ref() else {
        if let Some(error) = preflight.contract_error.as_deref() {
            model.compatibility = AdmissionStatus::Unsupported;
            model.metal_compatibility = AdmissionStatus::Unsupported;
            model.cuda_compatibility = AdmissionStatus::Unsupported;
            model.reason = format!("remote execution contract was rejected: {error}");
        } else {
            model.reason = format!(
                "remote headers inspected in {} bytes; task-specific admission is pending",
                preflight.fetched_bytes
            );
        }
        return;
    };
    let (metal, cuda) = admit(model, contract);
    model.features.vision = contract.vision().is_some();
    model.encoding = contract.checkpoint_encoding().label();
    let vision = contract.vision().map_or_else(String::new, |vision| {
        let processor = if vision.processor().is_some() {
            "ready"
        } else {
            "missing"
        };
        format!("; vision {}, processor {processor}", vision.readiness().summary())
    });
    model.reason = format!(
        "remote contract and {:?} tokenizer inspected in {} bytes; Metal {}, CUDA {}{}",
        preflight.metadata.tokenizer_assets.kind,
        preflight.fetched_bytes,
        metal.as_str(),
        cuda.as_str(),
        vision
    );
}

/// A GGUF variant admitted from its `fetched` header bytes; its encoding
/// stays the variant's own, since one variant's header stands for all.
pub(in crate::catalog) fn apply_gguf(
    model: &mut CatalogModel,
    contract: &RemoteModelContract,
    fetched: u64,
) {
    model.preflight_bytes = Some(fetched);
    let (metal, cuda) = admit(model, contract);
    model.reason = format!(
        "{}; GGUF header inspected in {fetched} bytes, Metal {}, CUDA {}",
        crate::catalog::fit::reason(model.compatibility, model.memory_fit),
        metal.as_str(),
        cuda.as_str()
    );
}

/// A GGUF variant whose header libmir refuses, such as for its tokenizer.
pub(in crate::catalog) fn reject_gguf(
    model: &mut CatalogModel,
    error: &libmir::Error,
    fetched: u64,
) {
    model.preflight_bytes = Some(fetched);
    model.preflight_error = Some(error.to_string());
    model.compatibility = AdmissionStatus::Unsupported;
    model.metal_compatibility = AdmissionStatus::Unsupported;
    model.cuda_compatibility = AdmissionStatus::Unsupported;
    model.confidence = Confidence::High;
    model.reason = format!("GGUF header was rejected: {error}");
}

/// Records each backend's admission and the best of them.
fn admit(
    model: &mut CatalogModel,
    contract: &RemoteModelContract,
) -> (AdmissionStatus, AdmissionStatus) {
    let metal = contract.admission(BackendTarget::Metal).status;
    let cuda = contract.admission(BackendTarget::Cuda).status;
    let overall = best_status(metal, cuda);
    model.compatibility = overall;
    model.metal_compatibility = metal;
    model.cuda_compatibility = cuda;
    model.confidence = if overall == AdmissionStatus::Unknown {
        Confidence::Medium
    } else {
        Confidence::High
    };
    (metal, cuda)
}

pub(in crate::catalog) fn failure(model: &mut CatalogModel, error: impl Into<String>) {
    let error = error.into();
    model.reason = format!("remote preflight unavailable: {error}");
    model.preflight_error = Some(error);
}

const fn best_status(metal: AdmissionStatus, cuda: AdmissionStatus) -> AdmissionStatus {
    if matches!(metal, AdmissionStatus::Supported) || matches!(cuda, AdmissionStatus::Supported) {
        AdmissionStatus::Supported
    } else if matches!(metal, AdmissionStatus::Partial) || matches!(cuda, AdmissionStatus::Partial)
    {
        AdmissionStatus::Partial
    } else if matches!(metal, AdmissionStatus::Unknown) || matches!(cuda, AdmissionStatus::Unknown)
    {
        AdmissionStatus::Unknown
    } else {
        AdmissionStatus::Unsupported
    }
}

#[cfg(test)]
mod tests;
