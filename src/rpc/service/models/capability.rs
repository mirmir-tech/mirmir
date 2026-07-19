use libmir::{Model, ModelDescriptor};

use crate::rpc::proto;

pub(super) fn model_info(id: String, path: String, model: &Model) -> proto::ModelInfo {
    let (image_input, image_unavailable_reason) = image_capability(model.descriptor());
    proto::ModelInfo {
        id,
        path,
        image_input,
        image_unavailable_reason,
    }
}

fn image_capability(descriptor: &ModelDescriptor) -> (bool, String) {
    if descriptor.vision().is_none() {
        return (
            false,
            "model configuration does not declare a supported image execution contract".to_owned(),
        );
    }
    match descriptor.vision_readiness() {
        Some(readiness) if !readiness.is_ready() => (
            false,
            format!("{} required vision tensors are missing", readiness.missing.len()),
        ),
        None => (false, "image execution contract readiness is unavailable".to_owned()),
        Some(_) if descriptor.image_processor().is_none() => {
            (false, "checkpoint does not provide a supported image processor".to_owned())
        },
        Some(_) => (true, String::new()),
    }
}
