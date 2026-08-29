use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use super::{CompletionMetrics, Snapshot, live, rates::Rates};

#[derive(Clone)]
pub struct Telemetry(Arc<Inner>);

struct Inner {
    started: Instant,
    total: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
    rates: Mutex<Rates>,
    live: live::Registry,
}

pub struct GenerationTelemetry {
    telemetry: Telemetry,
    live: live::Handle,
    finished: bool,
}

impl Telemetry {
    pub fn new() -> Self {
        Self(Arc::new(Inner {
            started: Instant::now(),
            total: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            prompt_tokens: AtomicU64::new(0),
            completion_tokens: AtomicU64::new(0),
            rates: Mutex::new(Rates::default()),
            live: live::Registry::new(),
        }))
    }

    pub fn begin(&self) -> GenerationTelemetry {
        self.0.total.fetch_add(1, Ordering::Relaxed);
        GenerationTelemetry {
            telemetry: self.clone(),
            live: self.0.live.begin(),
            finished: false,
        }
    }

    pub fn snapshot(&self, runtime: super::RuntimeTelemetry) -> Snapshot {
        let rates = self
            .0
            .rates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot();
        let live = self.0.live.snapshot();
        Snapshot {
            sampled_at_unix_ms: unix_ms(),
            uptime_ms: millis(self.0.started.elapsed()),
            loaded_models: runtime.loaded_models,
            active_requests: live.requests,
            total_requests: self.0.total.load(Ordering::Relaxed),
            completed_requests: self.0.completed.load(Ordering::Relaxed),
            failed_requests: self.0.failed.load(Ordering::Relaxed),
            prompt_tokens: self.0.prompt_tokens.load(Ordering::Relaxed),
            completion_tokens: self.0.completion_tokens.load(Ordering::Relaxed),
            last_tokens_per_second: rates.last_rate,
            mean_tokens_per_second: rates.mean_rate,
            last_ttft_ms: rates.last_ttft,
            mean_ttft_ms: rates.mean_ttft,
            host_total_memory_bytes: runtime.host_total,
            host_available_memory_bytes: runtime.host_available,
            memory_source: runtime.memory_source,
            kv_total_blocks: runtime.kv.total,
            kv_used_blocks: runtime.kv.used,
            kv_cached_prefixes: runtime.kv.prefixes,
            kv_hit_tokens: runtime.kv.hits,
            kv_miss_tokens: runtime.kv.misses,
            current_tokens_per_second: live.rate,
            current_prefill_tokens_per_second: live.prefill_rate,
            current_decode_tokens_per_second: live.decode_rate,
            current_ttft_ms: live.ttft_ms,
            active_elapsed_ms: live.elapsed_ms,
            active_prompt_tokens: live.prompt_tokens,
            active_completion_tokens: live.completion_tokens,
            active_stage: live.stage.map_or_else(String::new, |stage| stage.as_str().to_owned()),
            last_prefill_tokens_per_second: rates.last_prefill_rate,
            mean_prefill_tokens_per_second: rates.mean_prefill_rate,
            last_decode_tokens_per_second: rates.last_decode_rate,
            mean_decode_tokens_per_second: rates.mean_decode_rate,
            gpu_utilization_percent: runtime.device.utilization_percent,
            device_temperature_celsius: runtime.device.temperature_celsius,
            device_power_watts: runtime.device.power_watts,
            device_power_limit_watts: runtime.device.power_limit_watts,
            device_name: runtime.device.device_name,
        }
    }
}

impl GenerationTelemetry {
    pub fn resolving(&self) {
        self.live.resolving();
    }

    pub fn stage(&self, stage: libmir::ProgressStage) {
        self.live.stage(stage);
    }

    pub fn progress(&self, event: &libmir::ProgressEvent) {
        self.live.progress(event);
    }

    pub fn token_emitted(&self) {
        self.live.token_emitted();
    }

    pub fn complete(&mut self, completion: &CompletionMetrics) {
        self.telemetry.0.completed.fetch_add(1, Ordering::Relaxed);
        self.telemetry
            .0
            .prompt_tokens
            .fetch_add(completion.prompt_tokens, Ordering::Relaxed);
        self.telemetry
            .0
            .completion_tokens
            .fetch_add(completion.completion_tokens, Ordering::Relaxed);
        let mut rates =
            self.telemetry.0.rates.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        rates.record_rate(completion.tokens_per_second);
        rates.record_ttft(completion.ttft_ms);
        rates.record_prefill_rate(completion.prefill_tokens_per_second);
        rates.record_decode_rate(completion.decode_tokens_per_second);
        drop(rates);
        self.live.finish();
        self.finished = true;
    }

    pub fn fail(&mut self) {
        self.live.finish();
        self.finished = true;
        self.telemetry.0.failed.fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for GenerationTelemetry {
    fn drop(&mut self) {
        if !self.finished {
            self.live.finish();
            self.telemetry.0.failed.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
}

fn millis(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
