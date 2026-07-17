use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph, Sparkline},
};

use super::{app::App, theme};

const GIB: u64 = 1024 * 1024 * 1024;

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([Constraint::Length(4), Constraint::Length(8), Constraint::Min(7)])
        .split(area);
    cards(frame, rows[0], app);
    charts(frame, rows[1], app);
    models(frame, rows[2], app);
}

fn cards(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let cards = Layout::horizontal([
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
    ])
    .split(area);
    let telemetry = app.telemetry.as_ref();
    metric(
        frame,
        cards[0],
        "LOADED",
        &app.models.len().to_string(),
        loaded_detail(app.models.len()),
        theme::SUCCESS,
    );
    metric(
        frame,
        cards[1],
        "ACTIVE",
        &telemetry.map_or_else(|| "—".to_owned(), |value| value.active_requests.to_string()),
        &active_detail(telemetry),
        theme::GLACIER,
    );
    metric(
        frame,
        cards[2],
        "LAST E2E",
        &format_rate(telemetry.and_then(|value| value.last_tokens_per_second)),
        &telemetry.map_or_else(
            || "telemetry pending".to_owned(),
            |value| format!("mean {}", format_rate(value.mean_tokens_per_second)),
        ),
        theme::SIGNAL,
    );
    metric(
        frame,
        cards[3],
        "LAST TTFT",
        &format_ms(telemetry.and_then(|value| value.last_ttft_ms)),
        &telemetry.map_or_else(
            || "telemetry pending".to_owned(),
            |value| format!("mean {}", format_ms(value.mean_ttft_ms)),
        ),
        theme::GLACIER,
    );
}

fn charts(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let areas =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    let throughput = app.throughput_history.iter().copied().collect::<Vec<_>>();
    let memory = app.memory_history.iter().copied().collect::<Vec<_>>();
    sparkline(frame, areas[0], &throughput, &throughput_title(app), theme::GLACIER, None);
    sparkline(frame, areas[1], &memory, &memory_title(app), theme::SIGNAL, Some(100));
}

fn models(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app.models.iter().map(|model| {
        ListItem::new(Line::from(vec![
            Span::styled("● ", Style::new().fg(theme::SUCCESS)),
            Span::styled(&model.id, Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}", model.path), Style::new().fg(theme::MUTED)),
        ]))
    });
    let title = app.telemetry.as_ref().map_or_else(
        || " ACTIVE MODELS — telemetry pending ".to_owned(),
        |value| {
            format!(
                " ACTIVE MODELS — KV {}/{} blocks · requests {}/{} ok · {} failed{} ",
                value.kv_used_blocks,
                value.kv_total_blocks,
                value.completed_requests,
                value.total_requests,
                value.failed_requests,
                live_tokens(value),
            )
        },
    );
    frame.render_widget(
        List::new(items)
            .block(Block::bordered().title(title).border_style(Style::new().fg(theme::BORDER)))
            .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

fn metric(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    value: &str,
    detail: &str,
    color: ratatui::style::Color,
) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                value.to_owned(),
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(detail.to_owned(), Style::new().fg(theme::MUTED))),
        ])
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

fn sparkline(
    frame: &mut Frame<'_>,
    area: Rect,
    values: &[u64],
    title: &str,
    color: ratatui::style::Color,
    max: Option<u64>,
) {
    let maximum = max.unwrap_or_else(|| values.iter().copied().max().unwrap_or(1).max(1));
    frame.render_widget(
        Sparkline::default()
            .block(Block::bordered().title(title).border_style(Style::new().fg(theme::BORDER)))
            .data(values)
            .max(maximum)
            .style(Style::new().fg(color).bg(theme::SURFACE)),
        area,
    );
}

fn format_rate(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.1} tok/s"))
}

fn format_ms(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.1} ms"))
}

fn loaded_detail(count: usize) -> &'static str {
    ["no model ready", "model ready"].get(count).copied().unwrap_or("models ready")
}

fn active_detail(telemetry: Option<&crate::rpc::proto::TelemetrySnapshot>) -> String {
    telemetry.map_or_else(
        || "telemetry pending".to_owned(),
        |value| {
            if value.active_requests == 0 {
                "runtime idle".to_owned()
            } else {
                format!("{} · {}", value.active_stage, format_ms(Some(value.active_elapsed_ms)))
            }
        },
    )
}

fn throughput_title(app: &App) -> String {
    let Some(value) = &app.telemetry else {
        return " THROUGHPUT — telemetry pending ".to_owned();
    };
    let mean = format_rate(value.mean_tokens_per_second);
    if value.active_requests > 0 {
        format!(
            " THROUGHPUT — live {} · TTFT {} · completed mean {mean} ",
            format_rate(value.current_tokens_per_second),
            format_ms(value.current_ttft_ms),
        )
    } else {
        format!(" THROUGHPUT — completed mean {mean} ")
    }
}

fn live_tokens(value: &crate::rpc::proto::TelemetrySnapshot) -> String {
    if value.active_requests > 0 {
        format!(" · live {}p/{}c", value.active_prompt_tokens, value.active_completion_tokens)
    } else {
        String::new()
    }
}

fn memory_title(app: &App) -> String {
    let Some(telemetry) = &app.telemetry else {
        return " MEMORY — telemetry pending ".to_owned();
    };
    match (telemetry.host_total_memory_bytes, telemetry.host_available_memory_bytes) {
        (Some(total), Some(available)) => {
            format!(" MEMORY — {} / {} GiB used ", gib(total.saturating_sub(available)), gib(total))
        },
        _ => format!(" MEMORY — {} ", telemetry.memory_source),
    }
}

fn gib(bytes: u64) -> String {
    let whole = bytes / GIB;
    let tenth = bytes % GIB * 10 / GIB;
    format!("{whole}.{tenth}")
}
