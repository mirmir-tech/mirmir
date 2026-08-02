use libmir::{AdmissionStatus, foundation::model::BackendTarget};

use super::RemoteHeaderPreflight;
use crate::catalog::CatalogModel;

pub(in crate::catalog) fn apply(model: &mut CatalogModel, preflight: &RemoteHeaderPreflight) {
    model.preflight_bytes = Some(preflight.fetched_bytes);
    model.preflight_error.clone_from(&preflight.contract_error);
    let Some(contract) = preflight.contract.as_ref() else {
        if let Some(error) = preflight.contract_error.as_deref() {
            model.compatibility = "unsupported";
            model.metal_compatibility = "unsupported".into();
            model.cuda_compatibility = "unsupported".into();
            model.reason = format!("remote execution contract was rejected: {error}");
        } else {
            model.reason = format!(
                "remote headers inspected in {} bytes; task-specific admission is pending",
                preflight.fetched_bytes
            );
        }
        return;
    };
    let metal = contract.admission(BackendTarget::Metal).status;
    let cuda = contract.admission(BackendTarget::Cuda).status;
    model.features.vision = contract.vision().is_some();
    let overall = best_status(metal, cuda);
    model.compatibility = overall.as_str();
    model.metal_compatibility = metal.as_str().into();
    model.cuda_compatibility = cuda.as_str().into();
    model.encoding = contract.checkpoint_encoding().label();
    model.confidence = if overall == AdmissionStatus::Unknown {
        "medium"
    } else {
        "high"
    };
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
