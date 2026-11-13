use super::NativeRuntime;
use crate::application::{AcceleratorMemory, HostMemory, KvTelemetry, Result, RuntimeTelemetry};

impl NativeRuntime {
    pub fn telemetry(&self) -> Result<RuntimeTelemetry> {
        let state = self.lifecycle.state()?;
        let mut kv = KvTelemetry::default();
        for entry in state.resident.values() {
            kv.add(&entry.model.cache_stats());
        }
        let loaded_models = u64::try_from(state.resident.len()).unwrap_or(u64::MAX);
        drop(state);
        let accelerator_memory = self
            .library
            .memory_snapshot()
            .ok()
            .as_ref()
            .and_then(AcceleratorMemory::from_backend);
        let device = self.library.device_telemetry_snapshot().unwrap_or_default();
        Ok(RuntimeTelemetry {
            loaded_models,
            host_memory: HostMemory::sample(),
            accelerator_memory,
            kv,
            device,
        })
    }
}
