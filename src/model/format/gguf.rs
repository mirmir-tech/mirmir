//! The weight encoding of a local GGUF model, read from its header.

use std::path::Path;

use libmir::gguf::{GgmlType, GgufModel, files_in};

/// Types below this share of the payload (norms, biases) are left out.
const MIN_PERCENT: u64 = 1;

/// `Q4_K 74% · Q6_K 25%` for the GGUF model in `directory`, by share of
/// payload bytes; an unreadable header is named, not hidden.
pub(super) fn encoding(directory: &Path) -> String {
    let model = files_in(directory)
        .ok()
        .and_then(|files| files.into_iter().next())
        .map(GgufModel::open);
    match model {
        Some(Ok(model)) => describe(&model.type_bytes()),
        Some(Err(error)) => format!("Unreadable GGUF: {error}"),
        None => "Unknown GGUF".into(),
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
