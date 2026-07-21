use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use super::{format_ms, format_rate};
use crate::tui::{app::App, theme};

const GIB: u64 = 1024 * 1024 * 1024;

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([Constraint::Length(4), Constraint::Length(4)]).split(area);
    let telemetry = app.telemetry.as_ref();
    let values = [
        (
            "THROUGHPUT",
            format_rate(
                telemetry.and_then(|t| t.current_tokens_per_second.or(t.last_tokens_per_second)),
            ),
            format!("mean {}", format_rate(telemetry.and_then(|t| t.mean_tokens_per_second))),
            theme::GLACIER,
        ),
        (
            "PREFILL",
            format_rate(telemetry.and_then(|t| {
                t.current_prefill_tokens_per_second.or(t.last_prefill_tokens_per_second)
            })),
            format!(
                "mean {}",
                format_rate(telemetry.and_then(|t| t.mean_prefill_tokens_per_second))
            ),
            theme::SIGNAL,
        ),
        (
            "DECODE",
            format_rate(telemetry.and_then(|t| {
                t.current_decode_tokens_per_second.or(t.last_decode_tokens_per_second)
            })),
            format!(
                "mean {}",
                format_rate(telemetry.and_then(|t| t.mean_decode_tokens_per_second))
            ),
            theme::SUCCESS,
        ),
        (
            "TTFT",
            format_ms(telemetry.and_then(|t| t.current_ttft_ms.or(t.last_ttft_ms))),
            format!("mean {}", format_ms(telemetry.and_then(|t| t.mean_ttft_ms))),
            theme::GLACIER,
        ),
        ("MEMORY", memory_value(app), app.memory_source.clone(), theme::SIGNAL),
        ("K/V CACHE", kv_value(app), kv_detail(app), theme::SUCCESS),
    ];
    for (row, chunk) in rows.iter().enumerate() {
        let columns = Layout::horizontal([Constraint::Ratio(1, 3); 3]).split(*chunk);
        for column in 0..3 {
            let (label, value, detail, color) = &values[row * 3 + column];
            metric(frame, columns[column], label, value, detail, *color);
        }
    }
}

fn metric(frame: &mut Frame<'_>, area: Rect, label: &str, value: &str, detail: &str, color: Color) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {value} "), Style::new().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(detail.to_owned(), Style::new().fg(theme::MUTED)),
        ]))
        .alignment(Alignment::Center)
        .block(
            Block::bordered()
                .title(format!(" {label} "))
                .border_style(Style::new().fg(theme::BORDER)),
        )
        .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

fn memory_value(app: &App) -> String {
    let Some(telemetry) = &app.telemetry else {
        return "—".to_owned();
    };
    match (telemetry.host_total_memory_bytes, telemetry.host_available_memory_bytes) {
        (Some(total), Some(available)) => {
            format!("{}/{} GiB", gib(total.saturating_sub(available)), gib(total))
        },
        _ => "—".to_owned(),
    }
}

fn gib(bytes: u64) -> String {
    format!("{}.{:01}", bytes / GIB, bytes % GIB * 10 / GIB)
}

fn kv_value(app: &App) -> String {
    app.telemetry
        .as_ref()
        .map_or_else(|| "—".to_owned(), |t| format!("{}/{}", t.kv_used_blocks, t.kv_total_blocks))
}

fn kv_detail(app: &App) -> String {
    app.telemetry.as_ref().map_or_else(
        || "blocks".to_owned(),
        |t| format!("blocks · {} prefixes", t.kv_cached_prefixes),
    )
}
