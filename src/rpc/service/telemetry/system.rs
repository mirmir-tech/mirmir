use sysinfo::System;

#[derive(Default)]
pub struct Kv {
    pub total: u64,
    pub used: u64,
    pub prefixes: u64,
    pub hits: u64,
    pub misses: u64,
}

impl Kv {
    pub fn add(&mut self, stats: &libmir::CacheStats) {
        self.total = self.total.saturating_add(to_u64(stats.total_blocks));
        self.used = self.used.saturating_add(to_u64(stats.used_blocks));
        self.prefixes = self.prefixes.saturating_add(to_u64(stats.cached_prefixes));
        self.hits = self.hits.saturating_add(to_u64(stats.counters.hit_tokens));
        self.misses = self.misses.saturating_add(to_u64(stats.counters.miss_tokens));
    }
}

pub fn memory(library: &libmir::Library) -> (Option<u64>, Option<u64>, String) {
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
