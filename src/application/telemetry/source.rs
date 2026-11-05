use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use libmir::{CacheStats, CacheStatsReader, DeviceTelemetryReader};
use sysinfo::System;

use super::{KvTelemetry, RuntimeTelemetry};

/// Host-only observers: never owns a Model, Library, runtime port or native
/// handle.
#[derive(Clone, Default)]
pub struct Source(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    models: HashMap<String, (CacheStatsReader, Option<CacheStats>)>,
    device: DeviceTelemetryReader,
}

impl Source {
    pub fn new(device: DeviceTelemetryReader) -> Self {
        Self(Arc::new(Mutex::new(State { device, ..State::default() })))
    }

    pub fn insert(&self, id: String, cache: CacheStatsReader) {
        let mut state = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.models.insert(id, (cache, None));
    }

    pub fn remove(&self, id: &str) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .models
            .remove(id);
    }

    pub fn snapshot(&self) -> RuntimeTelemetry {
        let mut state = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let loaded_models = u64::try_from(state.models.len()).unwrap_or(u64::MAX);
        let mut kv = KvTelemetry::default();
        for (reader, previous) in state.models.values_mut() {
            if let Some(stats) = reader.snapshot() {
                *previous = Some(stats);
            }
            if let Some(stats) = previous.as_ref() {
                kv.add(stats);
            }
        }
        let device = state.device.clone();
        drop(state);
        let mut system = System::new();
        system.refresh_memory();
        let total = system.total_memory();
        RuntimeTelemetry {
            loaded_models,
            host_total: (total > 0).then_some(total),
            host_available: (total > 0).then_some(system.available_memory()),
            memory_source: if cfg!(target_os = "macos") {
                "macOS unified system memory"
            } else {
                "host system memory"
            }
            .into(),
            kv,
            device: device.snapshot(),
        }
    }
}
