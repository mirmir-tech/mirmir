use tonic::Status;

use super::RuntimeService;
pub use crate::application::SAMPLING_INTERVAL_MS;
use crate::{
    application::{
        AcceleratorMemory, AcceleratorMemoryPool, HistorySample, HostMemory,
        TELEMETRY_RETENTION_LIMIT, TelemetrySnapshot,
    },
    rpc::proto,
};

impl RuntimeService {
    pub(super) fn telemetry_snapshot(&self) -> Result<proto::TelemetrySnapshot, Status> {
        self.application
            .telemetry_snapshot()
            .map(Into::into)
            .map_err(super::status::application)
    }

    pub(crate) fn telemetry_recorder(&self) -> crate::application::TelemetryRecorder {
        self.application.telemetry_recorder()
    }

    pub(crate) fn telemetry_history_response(
        &self,
        limit: u32,
    ) -> Result<proto::TelemetryHistoryResponse, Status> {
        let samples =
            self.application.telemetry_history(limit).map_err(super::status::application)?;
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
            host_memory: value.host_memory.map(Into::into),
            accelerator_memory: value.accelerator_memory.map(Into::into),
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
            kv_total_blocks: value.kv_total_blocks,
            kv_used_blocks: value.kv_used_blocks,
            total_requests: value.total_requests,
            failed_requests: value.failed_requests,
            gpu_utilization_percent: value.gpu_utilization_percent,
            device_temperature_celsius: value.device_temperature_celsius,
            device_power_watts: value.device_power_watts,
            device_power_limit_watts: value.device_power_limit_watts,
            host_memory: value.host_memory.map(Into::into),
        }
    }
}

impl From<HostMemory> for proto::HostMemory {
    fn from(value: HostMemory) -> Self {
        Self {
            total_bytes: value.total_bytes,
            available_bytes: value.available_bytes,
        }
    }
}

impl From<AcceleratorMemory> for proto::AcceleratorMemory {
    fn from(value: AcceleratorMemory) -> Self {
        let pool = match value.pool {
            AcceleratorMemoryPool::MetalWorkingSet => proto::AcceleratorMemoryPool::MetalWorkingSet,
            AcceleratorMemoryPool::CudaDevice => proto::AcceleratorMemoryPool::CudaDevice,
            AcceleratorMemoryPool::CudaUnified => proto::AcceleratorMemoryPool::CudaUnified,
        };
        Self {
            pool: pool.into(),
            total_bytes: value.total_bytes,
            available_bytes: value.available_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_host_and_accelerator_memory_as_separate_typed_pools() {
        let snapshot = proto::TelemetrySnapshot::from(TelemetrySnapshot {
            host_memory: HostMemory::from_totals(64, 20),
            accelerator_memory: Some(AcceleratorMemory {
                pool: AcceleratorMemoryPool::MetalWorkingSet,
                total_bytes: 48,
                available_bytes: 30,
            }),
            ..Default::default()
        });
        assert_eq!(
            snapshot.host_memory,
            Some(proto::HostMemory { total_bytes: 64, available_bytes: 20 })
        );
        let accelerator = snapshot.accelerator_memory.expect("accelerator memory");
        assert_eq!(accelerator.pool(), proto::AcceleratorMemoryPool::MetalWorkingSet);
        assert_eq!((accelerator.total_bytes, accelerator.available_bytes), (48, 30));
        let sample = proto::TelemetryHistorySample::from(HistorySample {
            host_memory: HostMemory::from_totals(64, 20),
            ..Default::default()
        });
        assert_eq!(sample.host_memory, snapshot.host_memory);
    }
}
