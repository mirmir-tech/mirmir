use leptos::prelude::Set;

use crate::{
    state::{Page, RuntimeState},
    types::{Activity, Configuration, Model, Overview, Setting, TelemetryPoint},
};

pub fn populate(state: RuntimeState) {
    state.connection.set("connected".to_owned());
    state.server_version.set("0.3.0".to_owned());
    state.protocol_version.set("1".to_owned());
    state.overview.set(Some(overview()));
    state.telemetry.set((0_u64..140).map(telemetry_point).collect());
    state.activities.set(activities());
    state.models.set(models());
    state.models_loaded.set(true);
    state.configuration.set(Some(configuration()));
    state.page.set(capture_page());
}

fn capture_page() -> Page {
    let query = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .unwrap_or_default();
    match query.trim_start_matches('?').strip_prefix("page=") {
        Some("models") => Page::Models,
        Some("chat") => Page::Chat,
        Some("settings") => Page::Configuration,
        _ => Page::Overview,
    }
}

fn overview() -> Overview {
    Overview {
        sampled_at_unix_ms: 1_760_000_140_000,
        uptime_ms: 6_742_000,
        loaded_models: 1,
        active_requests: 1,
        total_requests: 128,
        completed_requests: 127,
        current_prefill_tokens_per_second: Some(611.7),
        current_decode_tokens_per_second: Some(42.6),
        current_ttft_ms: Some(184.0),
        last_prefill_tokens_per_second: Some(604.2),
        last_decode_tokens_per_second: Some(42.1),
        last_ttft_ms: Some(184.0),
        mean_prefill_tokens_per_second: Some(598.6),
        mean_decode_tokens_per_second: Some(41.4),
        mean_ttft_ms: Some(192.0),
        host_total_memory_bytes: Some(52 * 1024 * 1024 * 1024),
        host_available_memory_bytes: Some(33 * 1024 * 1024 * 1024),
        memory_source: "Apple unified memory".to_owned(),
        active_stage: "decode".to_owned(),
        gpu_utilization_percent: Some(72.0),
        device_temperature_celsius: Some(64.0),
        device_power_watts: Some(34.0),
        device_power_limit_watts: Some(48.0),
        device_name: "Apple M4 Max".to_owned(),
    }
}

const fn telemetry_point(index: u64) -> TelemetryPoint {
    TelemetryPoint {
        sampled_at_unix_ms: 1_760_000_000_000 + index * 1_000,
        memory_percent: Some((34 + index / 18 % 4 + noise(index / 9, 59, 2)) as f64),
        gpu_percent: Some((54 + noise(index, 71, 34)) as f64),
        temperature_celsius: Some((56 + noise(index / 5, 83, 13)) as f64),
        power_watts: Some((24 + noise(index, 97, 17)) as f64),
        power_limit_watts: Some(48.0),
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

fn models() -> Vec<Model> {
    vec![
        model("Qwen3-4B", "Qwen/Qwen3-4B", "active", "BF16", 8_032_000_000, true, true),
        model(
            "Gemma-4-4B-IT-4bit",
            "mlx-community/Gemma-4-4B-IT-4bit",
            "ready",
            "MLX affine 4-bit",
            3_240_000_000,
            true,
            true,
        ),
        model(
            "bge-reranker-v2-m3",
            "BAAI/bge-reranker-v2-m3",
            "available",
            "F16",
            1_120_000_000,
            false,
            false,
        ),
    ]
}

fn model(
    id: &str,
    repo_id: &str,
    state: &str,
    encoding: &str,
    size_bytes: u64,
    thinking: bool,
    vision: bool,
) -> Model {
    Model {
        id: id.to_owned(),
        repo_id: repo_id.to_owned(),
        revision: "main".to_owned(),
        commit: "capture".to_owned(),
        state: state.to_owned(),
        selector: repo_id.to_owned(),
        image_input: vision,
        ecosystem: "Transformers".to_owned(),
        container: "SafeTensors".to_owned(),
        encoding: encoding.to_owned(),
        metal_compatibility: "supported".to_owned(),
        cuda_compatibility: "supported".to_owned(),
        size_bytes,
        tool_use: thinking,
        thinking,
        vision,
        ..Default::default()
    }
}

fn configuration() -> Configuration {
    Configuration {
        values: vec![
            setting("server.http_bind", "127.0.0.1:8080", "config.toml", true, "text"),
            setting("server.web_enabled", "true", "config.toml", true, "boolean"),
            setting("runtime.max_concurrent", "4", "config.toml", false, "integer"),
            setting("runtime.kv_cache_tokens", "65536", "config.toml", false, "integer"),
            setting("generation.max_tokens", "1024", "default", false, "integer"),
            setting("generation.temperature", "0.7", "default", false, "float"),
            setting("hugging_face.token", "••••••••", "secrets.toml", false, "secret"),
        ],
        config_path: "~/.config/mirmir/config.toml".to_owned(),
        secrets_path: "~/.config/mirmir/secrets.toml".to_owned(),
        raw_toml: "[server]\nhttp_bind = \"127.0.0.1:8080\"\nweb_enabled = true\n\n[runtime]\nmax_concurrent = 4\nkv_cache_tokens = 65536\n".to_owned(),
    }
}

fn setting(key: &str, value: &str, source: &str, restart: bool, kind: &str) -> Setting {
    Setting {
        key: key.to_owned(),
        value: value.to_owned(),
        source: source.to_owned(),
        restart_required: restart,
        kind: kind.to_owned(),
        actions: vec!["edit".to_owned()],
    }
}
