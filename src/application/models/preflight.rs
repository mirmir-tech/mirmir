use libmir::ModelMemoryEstimate;

const GIB: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryFit {
    Fits,
    DoesNotFit,
    Unknown,
}

impl MemoryFit {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fits => "fits",
            Self::DoesNotFit => "does_not_fit",
            Self::Unknown => "unknown",
        }
    }
}

pub struct MemoryReport {
    pub estimate: ModelMemoryEstimate,
    pub available: Option<u64>,
    /// Bytes resident models' measured K/V caches would give back to this load.
    pub reclaimable: u64,
    pub budget: Option<u64>,
    pub source: String,
    pub fit: MemoryFit,
    pub max_safe_context: Option<u64>,
    pub(crate) capacity: Option<u64>,
}

pub enum Check {
    Ready,
    Pressure { message: String, eviction_can_help: bool },
}

pub fn rejection(estimate: ModelMemoryEstimate, budget: u64) -> String {
    let safe_context = safe_context(estimate, budget).unwrap_or(0);
    format!(
        "model needs about {} (weights {}, KV cache {}, workspace {}), but the safe device budget is {}; configured KV capacity is {} tokens and the estimated safe context is {} tokens; use --force to bypass this check",
        gib(estimate.required_bytes),
        gib(estimate.weight_bytes),
        gib(estimate.kv_cache_bytes),
        gib(estimate.workspace_bytes),
        gib(budget),
        estimate.cache_capacity_tokens,
        safe_context,
    )
}

pub fn safe_context(estimate: ModelMemoryEstimate, budget: u64) -> Option<u64> {
    (estimate.kv_bytes_per_token > 0).then(|| {
        (budget.saturating_sub(estimate.weight_bytes.saturating_add(estimate.workspace_bytes))
            / estimate.kv_bytes_per_token)
            .min(estimate.model_context_tokens)
            .min(estimate.cache_capacity_tokens)
    })
}

fn gib(bytes: u64) -> String {
    let hundredths = bytes.saturating_mul(100) / GIB;
    format!("{}.{:02} GiB", hundredths / 100, hundredths % 100)
}

pub fn eviction_can_help(required: u64, capacity: Option<u64>) -> bool {
    capacity.is_none_or(|capacity| required <= capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejection_includes_actionable_memory_breakdown() {
        let estimate = ModelMemoryEstimate {
            weight_bytes: 6 * GIB,
            kv_cache_bytes: 3 * GIB,
            workspace_bytes: GIB,
            required_bytes: 10 * GIB,
            kv_bytes_per_token: 1024 * 1024,
            cache_capacity_tokens: 3072,
            model_context_tokens: 8192,
            session_state_bytes: 0,
        };
        let message = rejection(estimate, 8 * GIB);
        assert!(message.contains("10.00 GiB"));
        assert!(message.contains("safe context is 1024 tokens"));
        assert!(message.contains("--force"));
    }

    #[test]
    fn eviction_is_skipped_when_the_model_exceeds_total_safe_capacity() {
        assert!(eviction_can_help(8 * GIB, Some(10 * GIB)));
        assert!(!eviction_can_help(12 * GIB, Some(10 * GIB)));
        assert!(eviction_can_help(12 * GIB, None));
    }
}
