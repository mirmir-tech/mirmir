//! The weight encoding of a local GGUF model, read from its header.

use std::path::Path;

use libmir::gguf::{GgmlType, text_model};

/// Types below this share of the payload (norms, biases) are left out.
const MIN_PERCENT: u64 = 1;

/// `Q4_K 74% · Q6_K 25%` for the text model in `directory`, by share of
/// payload bytes, never the vision projector linked beside it; an unreadable
/// header is named, not hidden.
pub(super) fn encoding(directory: &Path) -> String {
    match text_model(directory) {
        Ok(Some(model)) => describe(&model.type_bytes()),
        Ok(None) => "Unknown GGUF".into(),
        Err(error) => format!("Unreadable GGUF: {error}"),
    }
}

pub(super) fn describe(type_bytes: &[(GgmlType, u64)]) -> String {
    let total: u64 = type_bytes.iter().map(|entry| entry.1).sum();
    if total == 0 {
        return "Empty GGUF".into();
    }
    type_bytes
        .iter()
        .map(|&(kind, bytes)| (kind, bytes.saturating_mul(100) / total))
        .filter(|&(_, percent)| percent >= MIN_PERCENT)
        .map(|(kind, percent)| format!("{kind} {percent}%"))
        .collect::<Vec<_>>()
        .join(" · ")
}
