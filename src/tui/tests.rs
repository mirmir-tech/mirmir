use super::app::{App, Message, Screen};
mod benchmarks;
mod chat;
mod configuration;
mod fixtures;
mod help;
mod models;
mod restore;
use fixtures::{rendered, telemetry};
#[test]
fn renders_identity_dashboard_at_minimum_size() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(false);
    app.telemetry = Some(telemetry());
    app.throughput_history.extend([12, 24, 18]);
    app.memory_history.extend([40, 42, 41]);
    let text = rendered(&mut app, 100, 28)?;
    assert!(text.contains("MiRMiR"));
    assert!(text.contains("THROUGHPUT"));
    assert!(text.contains("24.5 tok/s"));
    assert!(text.contains("LAST E2E"));
    assert!(text.contains("ACTIVE") && text.contains("decode · 250.0 ms"));
    assert!(text.contains("mean 20.0 tok/s"));
    assert!(text.contains("live 20p/6c"));
    assert!(text.contains("KV 32/128 blocks"));
    assert!(text.contains("HEALTHY"));
    assert!(text.contains("LOCAL OWNER"));
    assert!(text.contains("[F1] Overview"));
    assert!(text.contains("[F2] Models"));
    assert!(text.contains("[F3] Chat"));
    assert!(text.contains("[F4] Settings"));
    assert!(text.contains("[F5] Activity"));
    assert!(text.contains("[F6] Bench"));
    assert!(!text.contains("Ctrl"));
    assert!(text.contains("? help"));
    Ok(())
}

#[test]
fn renders_activity_and_cancellation_capability() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Activity;
    app.activities.push(crate::rpc::proto::ActivityEvent {
        operation_id: "op-7".to_owned(),
        kind: "generate".to_owned(),
        target: "Qwen--Test".to_owned(),
        state: "running".to_owned(),
        stage: "decode".to_owned(),
        detail: "streaming tokens".to_owned(),
        started_at_unix_ms: 1,
        updated_at_unix_ms: 2,
        cancellable: true,
        current: None,
        total: None,
    });
    let text = rendered(&mut app, 110, 28)?;
    assert!(text.contains("ACTIVITY"));
    assert!(text.contains("Qwen--Test"));
    assert!(text.contains("CANCELLABLE yes"));
    Ok(())
}

#[test]
fn renders_streamed_chat_and_metrics() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Chat;
    app.models.push(crate::rpc::proto::ModelInfo {
        id: "Qwen--Test".to_owned(),
        path: "/models/qwen".to_owned(),
        ..Default::default()
    });
    app.chat_messages.push(Message {
        role: "user".to_owned(),
        content: "Hello".to_owned(),
        thought: String::new(),
    });
    app.chat_messages.push(Message {
        role: "assistant".to_owned(),
        content: "Hi from Mirmir".to_owned(),
        thought: String::new(),
    });
    app.chat_metrics = Some(crate::rpc::proto::Completion {
        reasoning: String::new(),
        text: "Hi from Mirmir".to_owned(),
        prompt_tokens: 3,
        completion_tokens: 4,
        finish_reason: "stop".to_owned(),
        elapsed_ms: 100.0,
        tokens_per_second: Some(40.0),
        ttft_ms: Some(12.5),
        prefill_tokens_per_second: Some(120.0),
        decode_tokens_per_second: Some(40.0),
        prefill_ms: Some(25.0),
        decode_ms: Some(75.0),
    });
    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("Qwen--Test"));
    assert!(text.contains("Hi from Mirmir"));
    assert!(text.contains("12.50 ms"));
    assert!(text.contains("120.00 tok/s"));
    assert!(text.contains("40.00 tok/s"));
    Ok(())
}

#[test]
fn renders_live_chat_metrics_during_decode() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Chat;
    app.chat_status = super::app::ChatStatus::Generating;
    app.chat_live_metrics = Some(super::app::ChatLiveMetrics {
        stage: "decode".to_owned(),
        elapsed_ms: 500.0,
        ttft_ms: Some(120.0),
        ttft_pending_ms: Some(120.0),
        prefill_tokens_per_second: Some(240.0),
        decode_tokens_per_second: Some(32.5),
        prompt_tokens: 20,
        completion_tokens: 8,
    });

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("120.00 ms"));
    assert!(text.contains("240.00 tok/s"));
    assert!(text.contains("32.50 tok/s"));
    assert!(text.contains("20p / 8c"));
    Ok(())
}

#[test]
fn renders_stage_only_during_model_loading() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.load_dialog = Some(super::app::LoadDialog {
        target: super::app::LoadTarget {
            selector: "Qwen--Test".to_owned(),
            config_id: "Qwen--Test".to_owned(),
            repo_id: "Qwen/Test".to_owned(),
            revision: "main".to_owned(),
            commit: "abc123".to_owned(),
        },
        status: super::app::LoadStatus::Loading,
        fields: ["2048", "0.7", "0.9", "40", "1.1"].map(str::to_owned),
        selected: 0,
        has_mirmir_overrides: true,
        memory: None,
        force: false,
        progress: Some(crate::rpc::proto::ModelLifecycleEvent {
            operation_id: "op-test".to_owned(),
            selector: "Qwen--Test".to_owned(),
            phase: "loading".to_owned(),
            current: 50,
            total: Some(100),
            unit: "byte".to_owned(),
            detail: "weights shard 1/2".to_owned(),
            model: None,
        }),
        error: None,
        restore: None,
    });

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("LOADING MODEL"));
    assert!(text.contains("MAPPING WEIGHT SHARDS"));
    assert!(text.contains("weights 50%"));
    assert!(text.contains("weights shard 1/2"));
    assert!(!text.contains("temperature"));
    Ok(())
}

#[test]
fn renders_thinking_state_and_locked_chat_input() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Chat;
    app.chat_status = super::app::ChatStatus::Generating;
    app.chat_messages.push(Message {
        role: "assistant".to_owned(),
        content: String::new(),
        thought: String::new(),
    });
    app.animation_tick = 40;
    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("Thinking"));
    assert!(text.contains("input locked"));
    Ok(())
}

#[test]
fn renders_chat_input_as_active_while_idle() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Chat;

    let text = rendered(&mut app, 100, 28)?;
    assert!(text.contains("Type a message"));
    assert!(text.contains("MESSAGE"));
    assert!(!text.contains("press i"));
    assert!(!text.contains("Esc navigation"));
    Ok(())
}

#[test]
fn renders_exit_confirmation() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.navigation.exit_dialog = true;

    let text = rendered(&mut app, 100, 28)?;
    assert!(text.contains("CONFIRM EXIT"));
    assert!(text.contains("Close Mirmir?"));
    Ok(())
}

#[test]
fn renders_external_cache_removal_confirmation() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.remove_dialog = Some(super::app::RemoveDialog {
        id: "Qwen--Test".to_owned(),
        repo_id: "Qwen/Test".to_owned(),
        path: "/cache/models--Qwen--Test/snapshots/abc123".to_owned(),
        source: "HF CACHE".to_owned(),
    });

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("CONFIRM MODEL REMOVAL"));
    assert!(text.contains("Remove Qwen--Test?"));
    assert!(text.contains("HF CACHE"));
    assert!(text.contains("permanently deleted"));
    assert!(text.contains("Enter / y"));
    assert!(text.contains("Esc / n"));
    Ok(())
}
