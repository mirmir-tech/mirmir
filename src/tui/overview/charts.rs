use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    widgets::{Block, Sparkline},
};

use crate::tui::{app::App, theme};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let columns = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
    ])
    .split(area);
    let e2e = app.telemetry_history.iter().map(|point| point.e2e).collect::<Vec<_>>();
    let prefill = app.telemetry_history.iter().map(|point| point.prefill).collect::<Vec<_>>();
    let decode = app.telemetry_history.iter().map(|point| point.decode).collect::<Vec<_>>();
    let memory = app
        .telemetry_history
        .iter()
        .map(|point| point.memory_percent)
        .collect::<Vec<_>>();
    let kv = app.telemetry_history.iter().map(|point| point.kv_percent).collect::<Vec<_>>();
    rates(frame, columns[0], &e2e, &prefill, &decode);
    spark(frame, columns[1], &memory, " MEMORY % ", theme::SIGNAL);
    spark(frame, columns[2], &kv, " K/V CACHE % ", theme::SUCCESS);
}

fn rates(frame: &mut Frame<'_>, area: Rect, e2e: &[u64], prefill: &[u64], decode: &[u64]) {
    let rows = Layout::vertical([Constraint::Ratio(1, 3); 3]).split(area);
    spark(frame, rows[0], e2e, " E2E TOK/S ", theme::GLACIER);
    spark(frame, rows[1], prefill, " PREFILL TOK/S ", theme::SIGNAL);
    spark(frame, rows[2], decode, " DECODE TOK/S ", theme::SUCCESS);
}

fn spark(
    frame: &mut Frame<'_>,
    area: Rect,
    values: &[u64],
    title: &str,
    color: ratatui::style::Color,
) {
    let maximum = values.iter().copied().max().unwrap_or(1).max(1);
    frame.render_widget(
        Sparkline::default()
            .block(Block::bordered().title(title).border_style(Style::new().fg(theme::BORDER)))
            .data(values)
            .max(maximum)
            .style(Style::new().fg(color).bg(theme::SURFACE)),
        area,
    );
}
