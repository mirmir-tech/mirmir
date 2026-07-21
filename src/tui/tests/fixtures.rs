use ratatui::{Terminal, backend::TestBackend};

use super::super::{app::App, render};

pub(super) fn rendered(
    app: &mut App,
    width: u16,
    height: u16,
) -> Result<String, std::convert::Infallible> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| render::draw(frame, app))?;
    Ok(terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect())
}

pub fn telemetry() -> crate::rpc::proto::TelemetrySnapshot {
    crate::rpc::proto::TelemetrySnapshot {
        sampled_at_unix_ms: 1,
        uptime_ms: 2,
        loaded_models: 0,
        active_requests: 1,
        total_requests: 4,
        completed_requests: 3,
        failed_requests: 1,
        prompt_tokens: 20,
        completion_tokens: 40,
        last_tokens_per_second: Some(24.5),
        mean_tokens_per_second: Some(20.0),
        last_ttft_ms: Some(12.0),
        mean_ttft_ms: Some(15.0),
        host_total_memory_bytes: Some(16 * 1024 * 1024 * 1024),
        host_available_memory_bytes: Some(8 * 1024 * 1024 * 1024),
        memory_source: "test unified memory".to_owned(),
        kv_total_blocks: 128,
        kv_used_blocks: 32,
        kv_cached_prefixes: 2,
        kv_hit_tokens: 100,
        kv_miss_tokens: 20,
        current_tokens_per_second: Some(24.5),
        current_prefill_tokens_per_second: None,
        current_decode_tokens_per_second: Some(31.0),
        current_ttft_ms: Some(12.0),
        active_elapsed_ms: 250.0,
        active_prompt_tokens: 20,
        active_completion_tokens: 6,
        active_stage: "decode".to_owned(),
        last_prefill_tokens_per_second: Some(120.0),
        mean_prefill_tokens_per_second: Some(110.0),
        last_decode_tokens_per_second: Some(31.0),
        mean_decode_tokens_per_second: Some(29.0),
    }
}
