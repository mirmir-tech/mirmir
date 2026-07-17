use libmir::Engine;

use crate::error::CliError;

pub(super) fn clear_after_emit(backend: &Engine, generated_tokens: usize) -> Result<(), CliError> {
    if !enabled() {
        return Ok(());
    }
    if generated_tokens == 0 {
        return Ok(());
    }
    if (generated_tokens - 1).is_multiple_of(256) {
        backend.clear_memory_cache()?;
    }
    Ok(())
}

fn enabled() -> bool {
    matches!(
        std::env::var("MIRMIR_METAL_CLEAR_CACHE").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "on" | "enabled" | "ENABLED")
    )
}
