use leptos::prelude::Set;

use crate::{
    state::RuntimeState,
    types::{Activity, Overview, TelemetryPoint},
};

pub fn populate(state: RuntimeState) {
    state.connection.set("connected".to_owned());
    state.server_version.set("0.2.0".to_owned());
    state.protocol_version.set("1".to_owned());
    state.overview.set(Some(overview()));
    state.telemetry.set((0_u64..140).map(telemetry_point).collect());
    state.activities.set(activities());
}

fn overview() -> Overview {
    Overview {
        sampled_at_unix_ms: 1_760_000_140_000,
        uptime_ms: 6_742_000,
        loaded_models: 1,
        active_requests: 1,
        total_requests: 128,
        failed_requests: 1,
        prompt_tokens: 184_320,
        completion_tokens: 36_864,
        current_tokens_per_second: Some(43.8),
        current_prefill_tokens_per_second: Some(611.7),
        current_decode_tokens_per_second: Some(42.6),
        current_ttft_ms: Some(184.0),
        last_tokens_per_second: Some(42.6),
        last_prefill_tokens_per_second: Some(604.2),
        last_decode_tokens_per_second: Some(42.1),
        last_ttft_ms: Some(184.0),
        host_total_memory_bytes: Some(52 * 1024 * 1024 * 1024),
        host_available_memory_bytes: Some(33 * 1024 * 1024 * 1024),
        memory_source: "Apple unified memory".to_owned(),
        kv_total_blocks: 4096,
        kv_used_blocks: 896,
        kv_cached_prefixes: 3,
        kv_hit_tokens: 41_472,
        kv_miss_tokens: 5_120,
    }
}

const fn telemetry_point(index: u64) -> TelemetryPoint {
    TelemetryPoint {
        sampled_at_unix_ms: 1_760_000_000_000 + index * 1_000,
        e2e: (37 + noise(index, 17, 13)) as f64,
        prefill: (540 + noise(index, 31, 150)) as f64,
        decode: (35 + noise(index, 47, 16)) as f64,
        memory_percent: (34 + index / 18 % 4 + noise(index / 9, 59, 2)) as f64,
        kv_percent: (14 + index % 90 / 6) as f64,
    }
}

const fn noise(index: u64, salt: u64, range: u64) -> u64 {
    let mut value = index.wrapping_add(salt.wrapping_mul(0x9e37_79b9));
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    value % range
}

fn activities() -> Vec<Activity> {
    [
        ("generate", "Qwen2.5-7B-Instruct", "running", "streaming tokens", Some(128)),
        ("generate", "Qwen2.5-7B-Instruct", "completed", "finished response", None),
        ("load", "Qwen2.5-7B-Instruct", "completed", "model ready", None),
        ("pull", "Qwen2.5-7B-Instruct", "completed", "checkpoint verified", None),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (kind, target, status, detail, current))| Activity {
        operation_id: format!("demo-{index}"),
        kind: kind.to_owned(),
        target: target.to_owned(),
        state: status.to_owned(),
        detail: detail.to_owned(),
        updated_at_unix_ms: 1_760_000_140_000 - index as u64 * 10_000,
        cancellable: status == "running",
        current,
        total: current.map(|_| 256),
    })
    .collect()
}
