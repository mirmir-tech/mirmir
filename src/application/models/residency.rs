use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug, Default)]
pub struct ModelResidency(Arc<AtomicU64>);

impl ModelResidency {
    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

pub fn eviction_candidate<'a>(
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
