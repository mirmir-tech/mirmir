use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tonic::Status;

use super::{ModelEntry, RuntimeService, lock};

#[derive(Clone, Debug, Default)]
pub(in crate::rpc::service) struct ModelResidency(Arc<AtomicU64>);

impl ModelResidency {
    pub(super) fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

impl RuntimeService {
    pub(super) fn resident_model(&self, key: &str) -> Result<Option<ModelEntry>, Status> {
        let mut models = lock(&self.models, "model registry")?;
        let Some(entry) = models.get_mut(key) else {
            return Ok(None);
        };
        entry.last_used = self.model_residency.next();
        let entry = entry.clone();
        drop(models);
        Ok(Some(entry))
    }

    pub(super) fn evict_lru_model(&self, protected: &str) -> Result<bool, Status> {
        let mut models = lock(&self.models, "model registry")?;
        let candidate = eviction_candidate(
            protected,
            models
                .iter()
                .map(|(key, entry)| (key.as_str(), entry.last_used, entry.model.is_in_use())),
        )
        .map(str::to_owned);
        let Some(key) = candidate else {
            return Ok(false);
        };
        let Some(entry) = models.remove(&key) else {
            return Ok(false);
        };
        drop(models);

        let model_id = entry.info.id.clone();
        if let Err(error) = entry.model.unload() {
            return Err(Status::internal(error.to_string()));
        }
        if let Err(error) = self.store.deactivate_model(&model_id) {
            tracing::warn!(%error, model = model_id, "failed to persist automatic model eviction");
        }
        tracing::info!(model = model_id, "evicted idle least-recently-used model");
        Ok(true)
    }
}

fn eviction_candidate<'a>(
    protected: &str,
    candidates: impl Iterator<Item = (&'a str, u64, bool)>,
) -> Option<&'a str> {
    candidates
        .filter(|(key, _, in_use)| *key != protected && !in_use)
        .min_by_key(|(_, last_used, _)| *last_used)
        .map(|(key, _, _)| key)
}

#[cfg(test)]
mod tests {
    use super::eviction_candidate;

    #[test]
    fn selects_oldest_idle_model_without_evicting_active_or_requested_models() {
        let candidates = [
            ("requested", 0, false),
            ("active", 1, true),
            ("oldest-idle", 2, false),
            ("newest-idle", 3, false),
        ];

        assert_eq!(eviction_candidate("requested", candidates.into_iter()), Some("oldest-idle"));
    }
}
