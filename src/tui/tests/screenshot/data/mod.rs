mod telemetry;

use telemetry::{points, snapshot};

use super::super::super::app::{App, InitialRefresh, Message, Screen};
use crate::rpc::proto::{
    ActivityEvent, Completion, ConfigurationSnapshot, ConfigurationValue, LocalModelInfo,
    ModelInfo, SecretState,
};

pub(super) fn demo_app(view: &str) -> App {
    let mut app = App::new(false);
    app.initial_refresh = InitialRefresh::Complete;
    app.server_version = "0.3.0".to_owned();
    app.protocol_version = "1".to_owned();
    app.memory_source = "Apple unified memory".to_owned();
    app.models.push(ModelInfo {
        id: "Qwen3-4B".to_owned(),
        path: "/models/Qwen3-4B".to_owned(),
        ..Default::default()
    });
    app.telemetry = Some(snapshot());
    app.telemetry_history = points();
    app.activities = activities();
    app.local_models = models();
    app.configuration = Some(configuration());
    seed_chat(&mut app);
    app.screen = match view {
        "models" => Screen::Models,
        "chat" => Screen::Chat,
        "settings" => Screen::Settings,
        _ => Screen::Dashboard,
    };
    app
}

fn seed_chat(app: &mut App) {
    app.chat_messages = vec![
        Message {
            role: "user".to_owned(),
            content: "Why keep the K/V cache on the accelerator?".to_owned(),
            thought: String::new(),
        },
        Message {
            role: "assistant".to_owned(),
            content: "It avoids host transfers during decode, lowers token latency, and lets concurrent requests reuse cached prefixes.".to_owned(),
            thought: "Connect latency, bandwidth, and prefix reuse.".to_owned(),
        },
    ];
    app.chat_metrics = Some(Completion {
        text: app.chat_messages[1].content.clone(),
        prompt_tokens: 18,
        completion_tokens: 21,
        finish_reason: "stop".to_owned(),
        elapsed_ms: 528.0,
        tokens_per_second: Some(39.8),
        ttft_ms: Some(184.0),
        prefill_tokens_per_second: Some(611.7),
        decode_tokens_per_second: Some(42.6),
        prefill_ms: Some(29.4),
        decode_ms: Some(492.0),
        reasoning: app.chat_messages[1].thought.clone(),
        tool_calls: Vec::new(),
    });
}

fn models() -> Vec<LocalModelInfo> {
    vec![
        model("Qwen3-4B", "Qwen/Qwen3-4B", "active", "BF16", 8_032_000_000, true),
        model(
            "Gemma-4-4B-IT-4bit",
            "mlx-community/Gemma-4-4B-IT-4bit",
            "available",
            "MLX affine 4-bit",
            3_240_000_000,
            true,
        ),
        model(
            "bge-reranker-v2-m3",
            "BAAI/bge-reranker-v2-m3",
            "available",
            "F16",
            1_120_000_000,
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
    features: bool,
) -> LocalModelInfo {
    LocalModelInfo {
        id: id.to_owned(),
        repo_id: repo_id.to_owned(),
        revision: "main".to_owned(),
        commit: "capture".to_owned(),
        path: format!("/models/{id}"),
        state: state.to_owned(),
        selector: repo_id.to_owned(),
        managed: true,
        image_input: features,
        model_class: "structurally admitted".to_owned(),
        loadable: true,
        library: "safetensors".to_owned(),
        size_bytes,
        tool_use: features,
        thinking: features,
        vision: features,
        ecosystem: "Transformers".to_owned(),
        container: "SafeTensors".to_owned(),
        encoding: encoding.to_owned(),
        metal_compatibility: "supported".to_owned(),
        cuda_compatibility: "supported".to_owned(),
        ..Default::default()
    }
}

fn configuration() -> ConfigurationSnapshot {
    ConfigurationSnapshot {
        values: [
            ("server.http_bind", "127.0.0.1:8080", true),
            ("server.web_enabled", "true", true),
            ("runtime.max_concurrent", "4", false),
            ("runtime.kv_cache_tokens", "65536", false),
            ("generation.max_tokens", "1024", false),
            ("generation.temperature", "0.7", false),
        ]
        .into_iter()
        .map(|(key, value, restart_required)| ConfigurationValue {
            key: key.to_owned(),
            value: value.to_owned(),
            source: if restart_required {
                "config.toml"
            } else {
                "default"
            }
            .to_owned(),
            editable: true,
            restart_required,
        })
        .collect(),
        hugging_face_token: Some(SecretState {
            configured: true,
            source: "secrets.toml".to_owned(),
        }),
        http_api_key: Some(SecretState {
            configured: true,
            source: "secrets.toml".to_owned(),
        }),
        config_path: "~/.config/mirmir/config.toml".to_owned(),
        secrets_path: "~/.config/mirmir/secrets.toml".to_owned(),
        raw_toml: "schema_version = 1".to_owned(),
    }
}

fn activities() -> Vec<ActivityEvent> {
    [
        ("generate", "Qwen3-4B", "running", "streaming token 128 of 256"),
        ("generate", "Qwen3-4B", "completed", "finished response"),
        ("load", "Qwen3-4B", "completed", "model ready"),
        ("pull", "Gemma-4-4B-IT-4bit", "completed", "checkpoint verified"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (kind, target, state, detail))| ActivityEvent {
        operation_id: format!("demo-{index}"),
        kind: kind.to_owned(),
        target: target.to_owned(),
        state: state.to_owned(),
        stage: if state == "running" {
            "decode"
        } else {
            "complete"
        }
        .to_owned(),
        detail: detail.to_owned(),
        started_at_unix_ms: 1_760_000_000_000 + index as u64 * 10_000,
        updated_at_unix_ms: 1_760_000_010_000 + index as u64 * 10_000,
        cancellable: state == "running",
        current: (state == "running").then_some(128),
        total: (state == "running").then_some(256),
    })
    .collect()
}
