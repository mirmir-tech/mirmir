use std::collections::VecDeque;

use super::super::super::super::app::TelemetryPoint;
use crate::rpc::proto::TelemetrySnapshot;

pub(super) fn snapshot() -> TelemetrySnapshot {
    TelemetrySnapshot {
        sampled_at_unix_ms: 1_760_000_140_000,
        uptime_ms: 6_742_000,
        loaded_models: 1,
        active_requests: 1,
        total_requests: 128,
        completed_requests: 127,
        failed_requests: 1,
        prompt_tokens: 184_320,
        completion_tokens: 36_864,
        last_tokens_per_second: Some(42.6),
        mean_tokens_per_second: Some(40.8),
        last_ttft_ms: Some(184.0),
        mean_ttft_ms: Some(196.4),
        host_total_memory_bytes: Some(52 * 1024 * 1024 * 1024),
        host_available_memory_bytes: Some(33 * 1024 * 1024 * 1024),
        memory_source: "Apple unified memory".to_owned(),
        kv_total_blocks: 4096,
        kv_used_blocks: 896,
        kv_cached_prefixes: 3,
        kv_hit_tokens: 41_472,
        kv_miss_tokens: 5_120,
        current_tokens_per_second: Some(43.8),
        current_prefill_tokens_per_second: Some(611.7),
        current_decode_tokens_per_second: Some(42.6),
        current_ttft_ms: Some(184.0),
        active_elapsed_ms: 2_900.0,
        active_prompt_tokens: 1_842,
        active_completion_tokens: 128,
        active_stage: "decode".to_owned(),
        last_prefill_tokens_per_second: Some(604.2),
        mean_prefill_tokens_per_second: Some(598.6),
        last_decode_tokens_per_second: Some(42.1),
        mean_decode_tokens_per_second: Some(41.4),
        gpu_utilization_percent: Some(72.0),
        device_temperature_celsius: Some(64.0),
        device_power_watts: Some(34.0),
        device_power_limit_watts: Some(48.0),
        device_name: "Apple M4 Max".to_owned(),
    }
}

pub(super) fn points() -> VecDeque<TelemetryPoint> {
    (0_u64..140).map(point).collect()
}

fn point(index: u64) -> TelemetryPoint {
    TelemetryPoint {
        memory_total_bytes: Some(64 * 1024 * 1024 * 1024),
        memory_used_bytes: Some((22 + index / 180 % 4) * 1024 * 1024 * 1024),
        memory_percent: Some(float(34 + index / 18 % 4 + noise(index / 9, 59, 2))),
        gpu_percent: Some(float(54 + noise(index, 71, 34))),
        temperature_celsius: Some(float(56 + noise(index / 5, 83, 13))),
        power_watts: Some(float(24 + noise(index, 97, 17))),
        power_limit_watts: Some(48.0),
    }
}

fn float(value: u64) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

const fn noise(index: u64, salt: u64, range: u64) -> u64 {
    let mut value = index.wrapping_add(salt.wrapping_mul(0x9e37_79b9));
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    value % range
}
