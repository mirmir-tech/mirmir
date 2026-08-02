use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use super::{format_ms, format_rate};
use crate::tui::{app::App, theme};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let telemetry = app.telemetry.as_ref();
    let values = [
        (
            "PREFILL",
            format_rate(telemetry.and_then(|value| {
                value.current_prefill_tokens_per_second.or(value.last_prefill_tokens_per_second)
            })),
            format!(
                "mean {}",
                format_rate(telemetry.and_then(|value| value.mean_prefill_tokens_per_second))
            ),
            theme::SIGNAL,
        ),
        (
            "DECODE",
            format_rate(telemetry.and_then(|value| {
                value.current_decode_tokens_per_second.or(value.last_decode_tokens_per_second)
            })),
            format!(
                "mean {}",
                format_rate(telemetry.and_then(|value| value.mean_decode_tokens_per_second))
            ),
            theme::SUCCESS,
        ),
        (
            "TTFT",
            format_ms(telemetry.and_then(|value| value.current_ttft_ms.or(value.last_ttft_ms))),
            format!("mean {}", format_ms(telemetry.and_then(|value| value.mean_ttft_ms))),
            theme::GLACIER,
        ),
    ];
    let columns = Layout::horizontal([Constraint::Ratio(1, 3); 3]).split(area);
    for (chunk, (label, value, detail, color)) in columns.iter().zip(values) {
        metric(frame, *chunk, label, &value, &detail, color);
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
