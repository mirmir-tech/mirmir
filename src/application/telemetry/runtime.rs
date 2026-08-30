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

impl KvTelemetry {
    pub(crate) fn add(&mut self, stats: &libmir::CacheStats) {
        self.total = self.total.saturating_add(to_u64(stats.total_blocks));
        self.used = self.used.saturating_add(to_u64(stats.used_blocks));
        self.prefixes = self.prefixes.saturating_add(to_u64(stats.cached_prefixes));
        self.hits = self.hits.saturating_add(to_u64(stats.counters.hit_tokens));
        self.misses = self.misses.saturating_add(to_u64(stats.counters.miss_tokens));
    }
}

fn to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
