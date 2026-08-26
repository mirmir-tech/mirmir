use tonic::Status;

use super::RuntimeService;
pub use crate::application::SAMPLING_INTERVAL_MS;
use crate::{
    application::{HistorySample, TELEMETRY_RETENTION_LIMIT, TelemetrySnapshot},
    rpc::proto,
};

impl RuntimeService {
    pub(super) fn telemetry_snapshot(&self) -> Result<proto::TelemetrySnapshot, Status> {
        self.application
            .telemetry_snapshot()
            .map(Into::into)
            .map_err(|error| Status::internal(error.to_string()))
    }

    pub(crate) fn record_telemetry_history(&self) -> Result<(), Status> {
        self.application
            .record_telemetry_history()
            .map_err(|error| Status::internal(error.to_string()))
    }

    pub(crate) fn flush_telemetry_history(&self) -> Result<(), Status> {
        self.application
            .flush_telemetry_history()
            .map_err(|error| Status::internal(error.to_string()))
    }

    pub(crate) fn telemetry_history_response(
        &self,
        limit: u32,
    ) -> Result<proto::TelemetryHistoryResponse, Status> {
        let samples = self
            .application
            .telemetry_history(limit)
            .map_err(|error| Status::internal(error.to_string()))?;
        Ok(proto::TelemetryHistoryResponse {
            samples: samples.into_iter().map(Into::into).collect(),
            retention_limit: u32::try_from(TELEMETRY_RETENTION_LIMIT).unwrap_or(u32::MAX),
            sampling_interval_ms: SAMPLING_INTERVAL_MS,
        })
    }
}

impl From<TelemetrySnapshot> for proto::TelemetrySnapshot {
    fn from(value: TelemetrySnapshot) -> Self {
        Self {
            sampled_at_unix_ms: value.sampled_at_unix_ms,
            uptime_ms: value.uptime_ms,
            loaded_models: value.loaded_models,
            active_requests: value.active_requests,
            total_requests: value.total_requests,
            completed_requests: value.completed_requests,
            failed_requests: value.failed_requests,
            prompt_tokens: value.prompt_tokens,
            completion_tokens: value.completion_tokens,
            last_tokens_per_second: value.last_tokens_per_second,
            mean_tokens_per_second: value.mean_tokens_per_second,
            last_ttft_ms: value.last_ttft_ms,
            mean_ttft_ms: value.mean_ttft_ms,
            host_total_memory_bytes: value.host_total_memory_bytes,
            host_available_memory_bytes: value.host_available_memory_bytes,
            memory_source: value.memory_source,
            kv_total_blocks: value.kv_total_blocks,
            kv_used_blocks: value.kv_used_blocks,
            kv_cached_prefixes: value.kv_cached_prefixes,
            kv_hit_tokens: value.kv_hit_tokens,
            kv_miss_tokens: value.kv_miss_tokens,
            current_tokens_per_second: value.current_tokens_per_second,
            current_prefill_tokens_per_second: value.current_prefill_tokens_per_second,
            current_decode_tokens_per_second: value.current_decode_tokens_per_second,
            current_ttft_ms: value.current_ttft_ms,
            active_elapsed_ms: value.active_elapsed_ms,
            active_prompt_tokens: value.active_prompt_tokens,
            active_completion_tokens: value.active_completion_tokens,
            active_stage: value.active_stage,
            last_prefill_tokens_per_second: value.last_prefill_tokens_per_second,
            mean_prefill_tokens_per_second: value.mean_prefill_tokens_per_second,
            last_decode_tokens_per_second: value.last_decode_tokens_per_second,
            mean_decode_tokens_per_second: value.mean_decode_tokens_per_second,
            gpu_utilization_percent: value.gpu_utilization_percent,
            device_temperature_celsius: value.device_temperature_celsius,
            device_power_watts: value.device_power_watts,
            device_power_limit_watts: value.device_power_limit_watts,
            device_name: value.device_name,
        }
    }
}

impl From<HistorySample> for proto::TelemetryHistorySample {
    fn from(value: HistorySample) -> Self {
        Self {
            sampled_at_unix_ms: value.sampled_at_unix_ms,
            loaded_models: value.loaded_models,
            active_requests: value.active_requests,
            e2e_tokens_per_second: value.e2e_tokens_per_second,
            prefill_tokens_per_second: value.prefill_tokens_per_second,
            decode_tokens_per_second: value.decode_tokens_per_second,
            ttft_ms: value.ttft_ms,
            memory_total_bytes: value.memory_total_bytes,
            memory_available_bytes: value.memory_available_bytes,
            memory_source: value.memory_source,
            kv_total_blocks: value.kv_total_blocks,
            kv_used_blocks: value.kv_used_blocks,
            total_requests: value.total_requests,
            failed_requests: value.failed_requests,
            gpu_utilization_percent: value.gpu_utilization_percent,
            device_temperature_celsius: value.device_temperature_celsius,
            device_power_watts: value.device_power_watts,
            device_power_limit_watts: value.device_power_limit_watts,
        }
    }
}
