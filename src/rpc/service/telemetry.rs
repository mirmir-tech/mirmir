use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use tonic::Status;

use super::RuntimeService;
use crate::rpc::proto;

pub(super) mod history;
mod live;
mod rates;
mod system;

use self::{
    history::History,
    live::{Handle as LiveHandle, Registry as LiveRegistry},
    rates::Rates,
    system::{Kv, memory},
};

#[derive(Clone)]
pub struct Telemetry(Arc<Inner>);

struct Inner {
    started: Instant,
    active: AtomicU64,
    total: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
    rates: Mutex<Rates>,
    live: LiveRegistry,
    history: History,
}

pub struct GenerationTelemetry {
    telemetry: Telemetry,
    live: LiveHandle,
    finished: bool,
}

impl Telemetry {
    pub fn new(history_path: PathBuf) -> Self {
        Self(Arc::new(Inner {
            started: Instant::now(),
            active: AtomicU64::new(0),
            total: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            prompt_tokens: AtomicU64::new(0),
            completion_tokens: AtomicU64::new(0),
            rates: Mutex::new(Rates::default()),
            live: LiveRegistry::new(),
            history: History::load(history_path),
        }))
    }

    pub fn begin(&self) -> GenerationTelemetry {
        self.0.active.fetch_add(1, Ordering::Relaxed);
        self.0.total.fetch_add(1, Ordering::Relaxed);
        GenerationTelemetry {
            telemetry: self.clone(),
            live: self.0.live.begin(),
            finished: false,
        }
    }
}

impl GenerationTelemetry {
    pub fn stage(&self, stage: &'static str) {
        self.live.stage(stage);
    }

    pub fn progress(&self, event: &libmir::ProgressEvent) {
        self.live.progress(event);
    }

    pub fn token_emitted(&self) {
        self.live.token_emitted();
    }

    pub fn complete(&mut self, completion: &proto::Completion) {
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
        self.telemetry.0.active.fetch_sub(1, Ordering::Relaxed);
    }
}

impl RuntimeService {
    pub(super) fn telemetry_snapshot(&self) -> Result<proto::TelemetrySnapshot, Status> {
        let models = super::status::lock(&self.models, "model registry")?;
        let mut kv = Kv::default();
        for entry in models.values() {
            kv.add(&entry.model.cache_stats());
        }
        let loaded_models = u64::try_from(models.len()).unwrap_or(u64::MAX);
        drop(models);
        let rates =
            self.telemetry.0.rates.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let rate_snapshot = rates.snapshot();
        drop(rates);
        let (host_total, host_available, memory_source) = memory(&self.library);
        let device = self.library.device_telemetry_snapshot().unwrap_or_default();
        let live = self.telemetry.0.live.snapshot();
        Ok(proto::TelemetrySnapshot {
            sampled_at_unix_ms: unix_ms(),
            uptime_ms: millis(self.telemetry.0.started.elapsed()),
            loaded_models,
            active_requests: live.requests,
            total_requests: self.telemetry.0.total.load(Ordering::Relaxed),
            completed_requests: self.telemetry.0.completed.load(Ordering::Relaxed),
            failed_requests: self.telemetry.0.failed.load(Ordering::Relaxed),
            prompt_tokens: self.telemetry.0.prompt_tokens.load(Ordering::Relaxed),
            completion_tokens: self.telemetry.0.completion_tokens.load(Ordering::Relaxed),
            last_tokens_per_second: rate_snapshot.last_rate,
            mean_tokens_per_second: rate_snapshot.mean_rate,
            last_ttft_ms: rate_snapshot.last_ttft,
            mean_ttft_ms: rate_snapshot.mean_ttft,
            host_total_memory_bytes: host_total,
            host_available_memory_bytes: host_available,
            memory_source,
            kv_total_blocks: kv.total,
            kv_used_blocks: kv.used,
            kv_cached_prefixes: kv.prefixes,
            kv_hit_tokens: kv.hits,
            kv_miss_tokens: kv.misses,
            current_tokens_per_second: live.rate,
            current_prefill_tokens_per_second: live.prefill_rate,
            current_decode_tokens_per_second: live.decode_rate,
            current_ttft_ms: live.ttft_ms,
            active_elapsed_ms: live.elapsed_ms,
            active_prompt_tokens: live.prompt_tokens,
            active_completion_tokens: live.completion_tokens,
            active_stage: live.stage,
            last_prefill_tokens_per_second: rate_snapshot.last_prefill_rate,
            mean_prefill_tokens_per_second: rate_snapshot.mean_prefill_rate,
            last_decode_tokens_per_second: rate_snapshot.last_decode_rate,
            mean_decode_tokens_per_second: rate_snapshot.mean_decode_rate,
            gpu_utilization_percent: device.utilization_percent,
            device_temperature_celsius: device.temperature_celsius,
            device_power_watts: device.power_watts,
            device_power_limit_watts: device.power_limit_watts,
            device_name: device.device_name,
        })
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
