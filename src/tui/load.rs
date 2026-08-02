mod capabilities;
mod memory;
mod slider;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Gauge, Paragraph},
};

use self::memory::memory_summary;
use super::{
    app::{App, LoadDialog, LoadStatus},
    theme,
};

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let Some(dialog) = app.load_dialog.as_ref() else {
        return;
    };
    if dialog.status == LoadStatus::Loading {
        draw_loading(frame, dialog, app.animation_tick);
        return;
    }
    let area = centered(frame.area(), 74, 20);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::bordered()
            .title(" LOAD MODEL ")
            .border_style(Style::new().fg(theme::GLACIER))
            .style(Style::new().bg(theme::SURFACE)),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin { horizontal: 2, vertical: 1 });
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(7),
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .split(inner);
    header(frame, rows[0], dialog);
    slider::draw(frame, rows[1], dialog);
    progress(frame, rows[2], dialog);
    error(frame, rows[3], dialog);
    hint(frame, rows[4], dialog);
}

fn draw_loading(frame: &mut Frame<'_>, dialog: &LoadDialog, animation_tick: u64) {
    let area = centered(frame.area(), 74, 12);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::bordered()
            .title(" LOADING MODEL ")
            .border_style(Style::new().fg(theme::SIGNAL))
            .style(Style::new().bg(theme::SURFACE)),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin { horizontal: 2, vertical: 1 });
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .split(inner);
    header(frame, rows[0], dialog);
    let (stage, detail, ratio, gauge) = loading_state(dialog);
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        [usize::try_from(animation_tick % 10).unwrap_or(0)];
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("{spinner} {stage}"),
            Style::new().fg(theme::SIGNAL),
        )])),
        rows[1],
    );
    frame.render_widget(
        Gauge::default()
            .block(Block::bordered().title(" STAGE PROGRESS "))
            .gauge_style(Style::new().fg(theme::SIGNAL).bg(theme::RAISED))
            .ratio(ratio)
            .label(gauge),
        rows[2],
    );
    frame.render_widget(Paragraph::new(detail).style(Style::new().fg(theme::MUTED)), rows[3]);
}

fn loading_state(dialog: &LoadDialog) -> (&'static str, String, f64, String) {
    let Some(event) = dialog.progress.as_ref() else {
        return (
            "STARTING",
            "Waiting for the server to begin model inspection".to_owned(),
            0.0,
            "starting".to_owned(),
        );
    };
    match event.phase.as_str() {
        "resolving" => (
            "RESOLVING CONFIGURATION",
            "Inspecting model metadata, tokenizer, and saved overrides".to_owned(),
            0.0,
            "metadata".to_owned(),
        ),
        "checking_memory" => (
            "CHECKING DEVICE MEMORY",
            event.detail.clone(),
            0.0,
            "memory preflight".to_owned(),
        ),
        "loading" => {
            let (ratio, percent) = event_ratio(event);
            (
                "MAPPING WEIGHT SHARDS",
                event.detail.clone(),
                ratio,
                format!("weights {percent}%"),
            )
        },
        "initializing" => (
            "INITIALIZING RUNTIME",
            "Weights are mapped; building backend execution state and warming kernels".to_owned(),
            1.0,
            "weights 100% · runtime initialization still active".to_owned(),
        ),
        _ => ("WORKING", event.detail.clone(), 0.0, event.phase.clone()),
    }
}

fn event_ratio(event: &crate::rpc::proto::ModelLifecycleEvent) -> (f64, u16) {
    let Some(total) = event.total.filter(|total| *total > 0) else {
        return (0.0, 0);
    };
    let current = event.current.min(total);
    let basis_points =
        u16::try_from(u128::from(current) * 10_000 / u128::from(total)).unwrap_or(10_000);
    (f64::from(basis_points) / 10_000.0, basis_points / 100)
}

fn header(frame: &mut Frame<'_>, area: Rect, dialog: &LoadDialog) {
    let source = match (dialog.task.as_str(), dialog.has_mirmir_overrides) {
        ("generation", true) => "Mirmir configuration",
        ("generation", false) => "model defaults",
        _ => "checkpoint capabilities",
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                &dialog.target.config_id,
                Style::new().fg(theme::INK).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  ·  {source}"), Style::new().fg(theme::MUTED)),
        ])),
        area,
    );
}

fn progress(frame: &mut Frame<'_>, area: Rect, dialog: &LoadDialog) {
    let (ratio, label) = match dialog.status {
        LoadStatus::Inspecting => (0.0, "inspecting model configuration".to_owned()),
        LoadStatus::Editing => memory_summary(dialog),
        LoadStatus::Loading => dialog.progress.as_ref().map_or_else(
            || (0.0, "starting model load".to_owned()),
            |event| {
                event.total.filter(|total| *total > 0).map_or_else(
                    || (0.0, format!("{} · {}", event.phase, event.detail)),
                    |total| {
                        let current = event.current.min(total);
                        let basis_points =
                            u16::try_from(u128::from(current) * 10_000 / u128::from(total))
                                .unwrap_or(10_000);
                        let ratio = f64::from(basis_points) / 10_000.0;
                        let percent = basis_points / 100;
                        (ratio, format!("{} {percent}% · {}", event.phase, event.detail))
                    },
                )
            },
        ),
    };
    frame.render_widget(
        Gauge::default()
            .block(Block::bordered().title(" PROGRESS "))
            .gauge_style(Style::new().fg(theme::SIGNAL).bg(theme::RAISED))
            .ratio(ratio.clamp(0.0, 1.0))
            .label(label),
        area,
    );
}

fn error(frame: &mut Frame<'_>, area: Rect, dialog: &LoadDialog) {
    let text = dialog.error.as_deref().unwrap_or_default();
    frame.render_widget(Paragraph::new(text).style(Style::new().fg(theme::DANGER)), area);
}

fn hint(frame: &mut Frame<'_>, area: Rect, dialog: &LoadDialog) {
    let text = match dialog.status {
        LoadStatus::Inspecting => "Reading checkpoint capabilities…",
        LoadStatus::Editing => "Review the resolved settings before loading",
        LoadStatus::Loading if dialog.task == "generation" => "Settings saved · loading",
        LoadStatus::Loading => "Loading checkpoint · no task settings",
    };
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(Style::new().fg(theme::MUTED)),
        area,
    );
}

fn centered(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = area.width.saturating_sub(4).min(max_width);
    let height = area.height.saturating_sub(2).min(max_height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
