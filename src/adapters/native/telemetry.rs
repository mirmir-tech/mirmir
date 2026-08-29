use sysinfo::System;

use super::NativeRuntime;
use crate::application::{KvTelemetry, Result, RuntimeTelemetry};

impl NativeRuntime {
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
