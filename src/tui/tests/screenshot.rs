use std::{env, fmt::Write as _, fs};

use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::{Buffer, Cell},
    style::{Color, Modifier},
};

use super::super::{
    app::{App, InitialRefresh, TelemetryPoint},
    render,
};
use crate::rpc::proto::{ActivityEvent, ModelInfo, TelemetrySnapshot};

const WIDTH: u16 = 160;
const HEIGHT: u16 = 45;

#[test]
#[ignore = "writes a browser-renderable capture fixture"]
fn writes_dashboard_capture() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::var("MIRMIR_TUI_CAPTURE_HTML")?;
    let mut app = demo_app();
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| render::draw(frame, &mut app))?;
    fs::write(output, capture_html(terminal.backend().buffer())?)?;
    Ok(())
}

fn demo_app() -> App {
    let mut app = App::new(false);
    app.initial_refresh = InitialRefresh::Complete;
    app.server_version = "0.2.0".to_owned();
    app.protocol_version = "1".to_owned();
    app.memory_source = "Apple unified memory".to_owned();
    app.models.push(ModelInfo {
        id: "Qwen2.5-7B-Instruct".to_owned(),
        ..Default::default()
    });
    app.telemetry = Some(demo_telemetry());
    app.telemetry_history = (0_u64..140).map(telemetry_point).collect();
    app.activities = demo_activities();
    app
}

fn demo_telemetry() -> TelemetrySnapshot {
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
    }
}

const fn telemetry_point(index: u64) -> TelemetryPoint {
    TelemetryPoint {
        e2e: 37 + noise(index, 17, 13),
        prefill: 540 + noise(index, 31, 150),
        decode: 35 + noise(index, 47, 16),
        memory_percent: 34 + index / 18 % 4 + noise(index / 9, 59, 2),
        kv_percent: 14 + index % 90 / 6,
    }
}

const fn noise(index: u64, salt: u64, range: u64) -> u64 {
    let mut value = index.wrapping_add(salt.wrapping_mul(0x9e37_79b9));
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    value % range
}

fn demo_activities() -> Vec<ActivityEvent> {
    [
        ("generate", "Qwen2.5-7B-Instruct", "completed", "finished response"),
        ("generate", "Qwen2.5-7B-Instruct", "running", "streaming tokens"),
        ("load", "Qwen2.5-7B-Instruct", "completed", "model ready"),
        ("pull", "Qwen2.5-7B-Instruct", "completed", "checkpoint verified"),
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
        current: None,
        total: None,
    })
    .collect()
}

fn capture_html(buffer: &Buffer) -> Result<String, std::fmt::Error> {
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><style>\
         *{box-sizing:border-box}html,body{margin:0;width:1600px;height:900px;overflow:hidden;\
         background:#0d1117}main{display:grid;grid-template-columns:repeat(160,10px);\
         grid-template-rows:repeat(45,20px);width:1600px;height:900px;\
         font:16px/20px 'JetBrains Mono',Menlo,monospace;font-variant-ligatures:none;\
         -webkit-font-smoothing:antialiased}span{display:block;width:10px;height:20px;overflow:visible;\
         white-space:pre}</style><main>",
    );
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let cell = buffer.cell((x, y)).expect("capture coordinates must be in bounds");
            write!(html, "<span style=\"{}\">{}</span>", cell_style(cell), escape(cell.symbol()))?;
        }
    }
    html.push_str("</main>");
    Ok(html)
}

fn cell_style(cell: &Cell) -> String {
    let mut style = format!("color:{};background:{}", foreground(cell.fg), background(cell.bg));
    if cell.modifier.contains(Modifier::BOLD) {
        style.push_str(";font-weight:700");
    }
    if cell.modifier.contains(Modifier::DIM) {
        style.push_str(";opacity:.65");
    }
    style
}

fn foreground(color: Color) -> String {
    color_value(color, "#e7ecef")
}

fn background(color: Color) -> String {
    color_value(color, "#0d1117")
}

fn color_value(color: Color, reset: &str) -> String {
    match color {
        Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
        Color::Reset => reset.to_owned(),
        other => other.to_string(),
    }
}

fn escape(symbol: &str) -> String {
    symbol.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
