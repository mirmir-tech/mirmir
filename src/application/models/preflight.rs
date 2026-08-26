use std::path::Path;

use libmir::{GenerationOverrides, ModelDescriptor, ModelMemoryEstimate};

use crate::application::{Result, RuntimeCoordinator};

const GIB: u64 = 1024 * 1024 * 1024;

pub struct MemoryReport {
    pub estimate: ModelMemoryEstimate,
    pub available: Option<u64>,
    pub budget: Option<u64>,
    pub source: String,
    pub fit: &'static str,
    pub max_safe_context: Option<u64>,
    capacity: Option<u64>,
}

pub(super) enum Check {
    Ready,
    Pressure { message: String, eviction_can_help: bool },
}

impl RuntimeCoordinator {
    pub(super) fn preflight(
        &self,
        path: &Path,
        overrides: GenerationOverrides,
        selector: &str,
        force: bool,
    ) -> Result<Check> {
        let descriptor = ModelDescriptor::inspect(path, overrides)?;
        let report = self.memory_report(&descriptor)?;
        let estimate = report.estimate;
        tracing::info!(
            model = selector,
            required_bytes = estimate.required_bytes,
            weight_bytes = estimate.weight_bytes,
            kv_cache_bytes = estimate.kv_cache_bytes,
            workspace_bytes = estimate.workspace_bytes,
            available_bytes = report.available,
            budget_bytes = report.budget,
            memory_source = report.source,
            fit = report.fit,
            max_safe_context = report.max_safe_context,
            forced = force,
            "model memory preflight"
        );
        let Some(budget) = report.budget else {
            tracing::warn!(model = selector, "memory preflight has no reliable device budget");
            return Ok(Check::Ready);
        };
        if estimate.required_bytes <= budget {
            return Ok(Check::Ready);
        }
        let message = rejection(estimate, budget);
        if force {
            tracing::warn!(model = selector, %message, "forcing model load despite memory preflight");
            Ok(Check::Ready)
        } else {
            Ok(Check::Pressure {
                message,
                eviction_can_help: eviction_can_help(estimate.required_bytes, report.capacity),
            })
        }
    }

    pub fn memory_report(&self, descriptor: &ModelDescriptor) -> Result<MemoryReport> {
        let config = self.library.model_config(descriptor)?;
        let target = self.library.backend_target()?;
        let estimate = descriptor.memory_estimate_for(&config, &target);
        let memory = self.library.memory_snapshot()?;
        let available =
            memory.available_bytes.map(|bytes| bytes.saturating_add(memory.cached_bytes));
        let reserve = self.library.config().memory.hard_reserve_bytes(&memory);
        let budget = available.map(|free| free.saturating_sub(reserve));
        let capacity = memory.total_bytes.map(|total| total.saturating_sub(reserve));
        let fit = match budget {
            Some(budget) if estimate.required_bytes <= budget => "fits",
            Some(_) => "does_not_fit",
            None => "unknown",
        };
        let max_safe_context = budget.and_then(|budget| safe_context(estimate, budget));
        Ok(MemoryReport {
            estimate,
            available,
            budget,
            source: memory.source,
            fit,
            max_safe_context,
            capacity,
        })
    }
}

fn rejection(estimate: ModelMemoryEstimate, budget: u64) -> String {
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

fn safe_context(estimate: ModelMemoryEstimate, budget: u64) -> Option<u64> {
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

fn eviction_can_help(required: u64, capacity: Option<u64>) -> bool {
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
