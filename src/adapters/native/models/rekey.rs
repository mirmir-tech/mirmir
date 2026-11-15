use super::super::NativeRuntime;
use crate::application::{Error, Result};

impl NativeRuntime {
    /// Moves a resident model to `new` when saving its first configuration
    /// gives it that id: a folder or cached checkpoint loaded by its path is
    /// resident under the path until then. Without this the next load would
    /// resolve the new id, miss the resident copy and load a second one,
    /// whose memory the first would keep until the server exits.
    pub(in crate::adapters::native) fn rekey_resident(&self, old: &str, new: &str) -> Result<()> {
        if old == new {
            return Ok(());
        }
        let mut state = self.lifecycle.state()?;
        let Some(mut entry) = state.resident.remove(old) else {
            return Ok(());
        };
        new.clone_into(&mut entry.info.id);
        let reader = entry.model.cache_stats_reader();
        state.resident.insert(new.to_owned(), entry);
        drop(state);
        self.telemetry_source.remove(old);
        self.telemetry_source.insert(new.to_owned(), reader);
        // Restored after a restart under the id it now resolves to.
        self.store
            .deactivate_model(old)
            .map_err(|error| Error::Persistence(error.to_string()))?;
        self.store
            .activate_model(new)
            .map_err(|error| Error::Persistence(error.to_string()))?;
        tracing::info!(from = old, to = new, "resident model moved to its new configuration");
        Ok(())
    }
}
