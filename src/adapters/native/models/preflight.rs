use std::path::Path;

use libmir::{GenerationOverrides, ModelDescriptor};

use super::super::NativeRuntime;
use crate::application::{
    Check, MemoryFit, MemoryReport, Result, eviction_can_help, rejection, safe_context,
};

impl NativeRuntime {
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
            reclaimable_kv_bytes = report.reclaimable,
            budget_bytes = report.budget,
            memory_source = report.source,
            fit = report.fit.as_str(),
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
        // Measured K/V caches of resident models hold memory they give back
        // to a new load, down to their minimum share; it is not gone.
        let reclaimable = self.library.reclaimable_kv_bytes()?;
        let budget = available.map(|free| free.saturating_add(reclaimable).saturating_sub(reserve));
        let capacity = memory.total_bytes.map(|total| total.saturating_sub(reserve));
        let fit = match budget {
            Some(budget) if estimate.required_bytes <= budget => MemoryFit::Fits,
            Some(_) => MemoryFit::DoesNotFit,
            None => MemoryFit::Unknown,
        };
        let max_safe_context = budget.and_then(|budget| safe_context(estimate, budget));
        Ok(MemoryReport {
            estimate,
            available,
            reclaimable,
            budget,
            source: memory.source,
            fit,
            max_safe_context,
            capacity,
        })
    }
}
