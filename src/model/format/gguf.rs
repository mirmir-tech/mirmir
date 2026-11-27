//! The weight encoding of a local GGUF model, read from its header.

use std::path::Path;

use libmir::gguf::text_model;

use super::ModelEncoding;

/// The GGML types of the text model in `directory`, never of the vision
/// projector linked beside it; an unreadable header is named, not hidden.
pub(super) fn encoding(directory: &Path) -> ModelEncoding {
    match text_model(directory) {
        Ok(Some(model)) => ModelEncoding::Ggml(model.type_bytes()),
        Ok(None) => ModelEncoding::NoGgufTextModel,
        Err(error) => ModelEncoding::UnreadableGguf(error.to_string()),
    }
}
