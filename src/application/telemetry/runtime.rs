use sysinfo::System;

use super::super::{Result, RuntimeCoordinator};

#[derive(Default)]
pub struct KvTelemetry {
    pub total: u64,
    pub used: u64,
    pub prefixes: u64,
    pub hits: u64,
    pub misses: u64,
}

pub struct RuntimeTelemetry {
    pub loaded_models: u64,
    pub host_total: Option<u64>,
    pub host_available: Option<u64>,
    pub memory_source: String,
    pub kv: KvTelemetry,
    pub device: libmir::DeviceTelemetrySnapshot,
}

impl RuntimeCoordinator {
    pub fn telemetry(&self) -> Result<RuntimeTelemetry> {
        let state = self.lifecycle.state()?;
        let mut kv = KvTelemetry::default();
        for entry in state.resident.values() {
            kv.add(&entry.model.cache_stats());
        }
        let loaded_models = u64::try_from(state.resident.len()).unwrap_or(u64::MAX);
        drop(state);
        let (host_total, host_available, memory_source) = memory(&self.library);
        let device = self.library.device_telemetry_snapshot().unwrap_or_default();
        Ok(RuntimeTelemetry {
            loaded_models,
            host_total,
            host_available,
            memory_source,
            kv,
            device,
        })
    }
}

impl KvTelemetry {
    fn add(&mut self, stats: &libmir::CacheStats) {
        self.total = self.total.saturating_add(to_u64(stats.total_blocks));
        self.used = self.used.saturating_add(to_u64(stats.used_blocks));
        self.prefixes = self.prefixes.saturating_add(to_u64(stats.cached_prefixes));
        self.hits = self.hits.saturating_add(to_u64(stats.counters.hit_tokens));
        self.misses = self.misses.saturating_add(to_u64(stats.counters.miss_tokens));
    }
}

fn memory(library: &libmir::Library) -> (Option<u64>, Option<u64>, String) {
    if let Ok(memory) = library.memory_snapshot() {
        return (memory.total_bytes, memory.available_bytes, memory.source);
    }
    let mut system = System::new();
    system.refresh_memory();
    let total = system.total_memory();
    let available = system.available_memory();
    let source = if cfg!(target_os = "macos") {
        "macOS unified system memory"
    } else {
        "host memory; accelerator telemetry unavailable"
    };
    (
        (total > 0).then_some(total),
        (total > 0).then_some(available),
        source.to_owned(),
    )
}

fn to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
