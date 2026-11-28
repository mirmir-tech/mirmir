//! The weight encoding of a local GGUF model, read from its header.

use std::path::Path;

use libmir::gguf::{GgufModel, text_model};

use super::ModelEncoding;

/// The GGML types of the text model at `path` (a direct `.gguf` file or a
/// directory), never of the vision projector beside it; an unreadable header is
/// named, not hidden.
pub(super) fn encoding(path: &Path) -> ModelEncoding {
    if path.is_file() {
        return match GgufModel::open(path) {
            Ok(model) => ModelEncoding::Ggml(model.type_bytes()),
            Err(error) => ModelEncoding::UnreadableGguf(error.to_string()),
        };
    }
    match text_model(path) {
        Ok(Some(model)) => ModelEncoding::Ggml(model.type_bytes()),
        Ok(None) => ModelEncoding::NoGgufTextModel,
        Err(error) => ModelEncoding::UnreadableGguf(error.to_string()),
    }
}
