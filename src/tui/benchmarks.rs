use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use super::{
    app::{App, BenchmarkStatus},
    theme,
};
use crate::prompt::benchmark::Distribution;

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(3),
    ])
    .split(area);
    model(frame, rows[0], app);
    configuration(frame, rows[1], app);
    results(frame, rows[2], app);
    prompt(frame, rows[3], app);
}

fn model(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let selected = app
        .models
        .get(app.benchmark.model_index)
        .map_or("none loaded", |model| model.id.as_str());
    let running = app.benchmark.status == BenchmarkStatus::Running;
    let (status, color) = if running {
        ("RUNNING", theme::SIGNAL)
    } else {
        ("READY", theme::SUCCESS)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(selected, Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(status, Style::new().fg(color).add_modifier(Modifier::BOLD)),
        ]))
        .block(
            Block::bordered()
                .title(" BENCHMARK MODEL ")
                .border_style(Style::new().fg(theme::BORDER)),
        )
        .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

fn configuration(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let progress = if app.benchmark.status == BenchmarkStatus::Running {
        format!(
            " · {} · {}/{} complete",
            app.benchmark.phase, app.benchmark.completed, app.benchmark.samples
        )
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(format!(
            " warmup {} · samples {} · max tokens 128 · seed 42{progress}",
            app.benchmark.warmup, app.benchmark.samples,
        ))
        .block(
            Block::bordered()
                .title(" RUN CONFIGURATION ")
                .border_style(Style::new().fg(theme::BORDER)),
        )
        .style(Style::new().fg(theme::MUTED).bg(theme::SURFACE)),
        area,
    );
}

fn results(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let columns =
        Layout::horizontal([Constraint::Percentage(66), Constraint::Percentage(34)]).split(area);
    aggregate(frame, columns[0], app);
    history(frame, columns[1], app);
}

fn aggregate(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = vec![Line::from(vec![
        cell("metric", 13, theme::MUTED),
        cell("mean", 12, theme::MUTED),
        cell("median", 12, theme::MUTED),
        cell("p95", 12, theme::MUTED),
        cell("stddev", 12, theme::MUTED),
    ])];
    if let Some(aggregate) = &app.benchmark.aggregate {
        for (name, values, unit) in aggregate.metrics() {
            if let Some(values) = values {
                lines.push(metric(name, values, unit));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Run a benchmark to collect server-side metrics.",
            Style::new().fg(theme::MUTED),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::bordered()
                    .title(" DISTRIBUTIONS ")
                    .border_style(Style::new().fg(theme::BORDER)),
            )
            .style(Style::new().fg(theme::INK).bg(theme::SURFACE)),
        area,
    );
}

fn history(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = Vec::new();
    for report in app.benchmark.runs.iter().rev().take(8) {
        let decode = report
            .aggregate
            .decode_tokens_per_second
            .as_ref()
            .map_or_else(|| "n/a".to_owned(), |value| format!("{:.1}", value.median));
        let ttft = report
            .aggregate
            .ttft_ms
            .as_ref()
            .map_or_else(|| "n/a".to_owned(), |value| format!("{:.1}", value.median));
        lines.push(Line::from(Span::styled(&report.model, Style::new().fg(theme::GLACIER))));
        lines.push(Line::from(Span::styled(
            format!("  decode {decode} tok/s · TTFT {ttft} ms"),
            Style::new().fg(theme::MUTED),
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("No runs yet.", Style::new().fg(theme::MUTED))));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::bordered()
                    .title(" RUN HISTORY ")
                    .border_style(Style::new().fg(theme::BORDER)),
            )
            .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

fn prompt(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let locked = app.benchmark.status == BenchmarkStatus::Running;
    let text = if app.benchmark.prompt.is_empty() {
        "Type a repeatable benchmark prompt…"
    } else {
        &app.benchmark.prompt
    };
    let error = app
        .benchmark
        .error
        .as_deref()
        .map_or(String::new(), |value| format!(" · {value}"));
    frame.render_widget(
        Paragraph::new(format!(
            "{text}{}{error}",
            if locked {
                ""
            } else {
                "▌"
            }
        ))
        .block(Block::bordered().title(" PROMPT ").border_style(Style::new().fg(if locked {
            theme::MUTED
        } else {
            theme::GLACIER
        })))
        .style(
            Style::new()
                .fg(if app.benchmark.prompt.is_empty() {
                    theme::MUTED
                } else {
                    theme::INK
                })
                .bg(theme::SURFACE),
        ),
        area,
    );
}

fn metric(name: &str, values: &Distribution, unit: &str) -> Line<'static> {
    Line::from(vec![
        cell(name, 13, theme::GLACIER),
        cell(&format!("{:.2} {unit}", values.mean), 12, theme::INK),
        cell(&format!("{:.2}", values.median), 12, theme::INK),
        cell(&format!("{:.2}", values.p95), 12, theme::INK),
        cell(&format!("{:.2}", values.stddev), 12, theme::INK),
    ])
}

fn cell(value: &str, width: usize, color: Color) -> Span<'static> {
    Span::styled(format!("{value:<width$}"), Style::new().fg(color))
}
