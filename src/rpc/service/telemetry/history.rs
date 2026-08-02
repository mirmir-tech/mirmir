use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

use crate::{
    config::write_toml,
    error::{Error, Result},
    rpc::proto,
};

pub const RETENTION_LIMIT: usize = 900;
pub const SAMPLING_INTERVAL_MS: u64 = 1_000;
const FILE_VERSION: u32 = 1;
const FLUSH_INTERVAL: usize = 5;

#[derive(Clone)]
pub struct History(Arc<Inner>);

struct Inner {
    path: PathBuf,
    state: Mutex<State>,
}

struct State {
    samples: VecDeque<proto::TelemetryHistorySample>,
    dirty: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryFile {
    schema_version: u32,
    samples: Vec<proto::TelemetryHistorySample>,
}

impl History {
    pub fn load(path: PathBuf) -> Self {
        let samples = load_samples(&path).unwrap_or_else(|error| {
            tracing::warn!(%error, path = %path.display(), "telemetry history could not be loaded");
            VecDeque::new()
        });
        Self(Arc::new(Inner {
            path,
            state: Mutex::new(State { samples, dirty: 0 }),
        }))
    }

    pub fn record(&self, snapshot: &proto::TelemetrySnapshot) -> Result<()> {
        let Ok(mut state) = self.0.state.lock() else {
            return Err(poisoned());
        };
        state.samples.push_back(sample(snapshot));
        trim(&mut state.samples);
        state.dirty = state.dirty.saturating_add(1);
        if state.dirty >= FLUSH_INTERVAL {
            persist(&self.0.path, &state.samples)?;
            state.dirty = 0;
        }
        drop(state);
        Ok(())
    }

    pub fn response(&self, requested_limit: u32) -> Result<proto::TelemetryHistoryResponse> {
        let Ok(state) = self.0.state.lock() else {
            return Err(poisoned());
        };
        let requested = usize::try_from(requested_limit).unwrap_or(RETENTION_LIMIT);
        let limit = if requested == 0 {
            RETENTION_LIMIT
        } else {
            requested.min(RETENTION_LIMIT)
        };
        let skip = state.samples.len().saturating_sub(limit);
        Ok(proto::TelemetryHistoryResponse {
            samples: state.samples.iter().skip(skip).cloned().collect(),
            retention_limit: u32::try_from(RETENTION_LIMIT).unwrap_or(u32::MAX),
            sampling_interval_ms: SAMPLING_INTERVAL_MS,
        })
    }

    pub fn flush(&self) -> Result<()> {
        let Ok(mut state) = self.0.state.lock() else {
            return Err(poisoned());
        };
        if state.dirty == 0 {
            return Ok(());
        }
        persist(&self.0.path, &state.samples)?;
        state.dirty = 0;
        drop(state);
        Ok(())
    }
}

fn sample(snapshot: &proto::TelemetrySnapshot) -> proto::TelemetryHistorySample {
    proto::TelemetryHistorySample {
        sampled_at_unix_ms: snapshot.sampled_at_unix_ms,
        loaded_models: snapshot.loaded_models,
        active_requests: snapshot.active_requests,
        e2e_tokens_per_second: finite(
            snapshot.current_tokens_per_second.or(snapshot.last_tokens_per_second),
        ),
        prefill_tokens_per_second: finite(
            snapshot
                .current_prefill_tokens_per_second
                .or(snapshot.last_prefill_tokens_per_second),
        ),
        decode_tokens_per_second: finite(
            snapshot
                .current_decode_tokens_per_second
                .or(snapshot.last_decode_tokens_per_second),
        ),
        ttft_ms: finite(snapshot.current_ttft_ms.or(snapshot.last_ttft_ms)),
        memory_total_bytes: snapshot.host_total_memory_bytes,
        memory_available_bytes: snapshot.host_available_memory_bytes,
        memory_source: snapshot.memory_source.clone(),
        kv_total_blocks: snapshot.kv_total_blocks,
        kv_used_blocks: snapshot.kv_used_blocks,
        total_requests: snapshot.total_requests,
        failed_requests: snapshot.failed_requests,
        gpu_utilization_percent: finite(snapshot.gpu_utilization_percent),
        device_temperature_celsius: finite(snapshot.device_temperature_celsius),
        device_power_watts: finite(snapshot.device_power_watts),
        device_power_limit_watts: finite(snapshot.device_power_limit_watts),
    }
}

fn load_samples(path: &Path) -> Result<VecDeque<proto::TelemetryHistorySample>> {
    if !path.exists() {
        return Ok(VecDeque::new());
    }
    let file: HistoryFile = toml::from_str(&fs::read_to_string(path)?)?;
    if file.schema_version != FILE_VERSION {
        return Err(Error::Config(format!(
            "unsupported telemetry history schema {}",
            file.schema_version
        )));
    }
    let mut samples = VecDeque::from(file.samples);
    trim(&mut samples);
    Ok(samples)
}

fn persist(path: &Path, samples: &VecDeque<proto::TelemetryHistorySample>) -> Result<()> {
    write_toml(
        path,
        &HistoryFile {
            schema_version: FILE_VERSION,
            samples: samples.iter().cloned().collect(),
        },
        true,
    )
}

fn trim(samples: &mut VecDeque<proto::TelemetryHistorySample>) {
    while samples.len() > RETENTION_LIMIT {
        let _oldest = samples.pop_front();
    }
}

fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

fn poisoned() -> Error {
    Error::Config("telemetry history lock is poisoned".to_owned())
}

impl super::super::RuntimeService {
    pub(crate) fn record_telemetry_history(&self) -> std::result::Result<(), tonic::Status> {
        let snapshot = self.telemetry_snapshot()?;
        super::super::status::internal(self.telemetry.0.history.record(&snapshot))
    }

    pub(crate) fn flush_telemetry_history(&self) -> std::result::Result<(), tonic::Status> {
        super::super::status::internal(self.telemetry.0.history.flush())
    }

    pub(crate) fn telemetry_history_response(
        &self,
        limit: u32,
    ) -> std::result::Result<proto::TelemetryHistoryResponse, tonic::Status> {
        super::super::status::internal(self.telemetry.0.history.response(limit))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_HISTORY: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn persists_and_restores_bounded_history() -> Result<()> {
        let path = path();
        let history = History::load(path.clone());
        history.record(&snapshot(1, 12.0))?;
        history.record(&snapshot(2, 24.0))?;
        history.flush()?;
        let restored = History::load(path.clone()).response(1)?;
        assert_eq!(restored.samples.len(), 1);
        assert_eq!(restored.samples[0].sampled_at_unix_ms, 2);
        assert_eq!(restored.samples[0].e2e_tokens_per_second, Some(24.0));
        assert_eq!(restored.samples[0].prefill_tokens_per_second, Some(48.0));
        assert_eq!(restored.samples[0].decode_tokens_per_second, Some(12.0));
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn trims_oldest_samples() {
        let mut samples = (0..=RETENTION_LIMIT)
            .map(|index| proto::TelemetryHistorySample {
                sampled_at_unix_ms: u64::try_from(index).unwrap_or(u64::MAX),
                ..Default::default()
            })
            .collect();
        trim(&mut samples);
        assert_eq!(samples.len(), RETENTION_LIMIT);
        assert_eq!(samples.front().map(|sample| sample.sampled_at_unix_ms), Some(1));
    }

    fn path() -> PathBuf {
        let id = NEXT_HISTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("mirmir-telemetry-{}-{id}.toml", std::process::id()))
    }

    fn snapshot(sampled_at_unix_ms: u64, rate: f64) -> proto::TelemetrySnapshot {
        proto::TelemetrySnapshot {
            sampled_at_unix_ms,
            current_tokens_per_second: Some(rate),
            last_prefill_tokens_per_second: Some(rate * 2.0),
            last_decode_tokens_per_second: Some(rate / 2.0),
            memory_source: "test".to_owned(),
            ..Default::default()
        }
    }
}
