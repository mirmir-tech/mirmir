use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Row, Table, TableState},
};

use super::{
    super::{app::App, theme},
    bytes,
};
use crate::rpc::proto;

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app.visible_local_models().map(|model| {
        let state = model_state(app, model);
        let row = Row::new(vec![
            Cell::from(model.id.clone()),
            Cell::from(type_badge(model)),
            Cell::from(bytes(model.size_bytes)),
            Cell::from(features(model)),
            Cell::from(Line::from(Span::styled(
                state.clone(),
                Style::new().fg(state_color(&state)),
            ))),
            Cell::from(actions(&state, model.managed)),
        ]);
        if state == "REMOVING" {
            row.style(Style::new().fg(theme::MUTED))
        } else {
            row
        }
    });
    let header = Row::new(["NAME", "TYPE", "SIZE", "FEATURES", "STATE", "ACTIONS"])
        .style(Style::new().fg(theme::MUTED).add_modifier(Modifier::BOLD));
    let widths = [
        Constraint::Percentage(32),
        Constraint::Length(10),
        Constraint::Length(11),
        Constraint::Percentage(24),
        Constraint::Length(13),
        Constraint::Length(18),
    ];
    let selected = (app.local_model_count() > 0).then_some(app.local_selected);
    let mut state = TableState::default().with_selected(selected);
    frame.render_stateful_widget(
        Table::new(rows, widths)
            .header(header)
            .row_highlight_style(Style::new().bg(theme::RAISED).fg(theme::INK))
            .block(
                Block::bordered()
                    .title(" LOCAL MODELS ")
                    .border_style(Style::new().fg(theme::BORDER)),
            ),
        area,
        &mut state,
    );
}

fn model_state(app: &App, model: &proto::LocalModelInfo) -> String {
    if app.activities.iter().any(|event| {
        (event.target == model.id || event.target == model.repo_id)
            && matches!(event.state.as_str(), "queued" | "running" | "cancelling")
            && (event.kind.contains("load")
                || event.kind.contains("pull")
                || event.kind.contains("download"))
    }) {
        return "◌ WORKING".to_owned();
    }
    if model.state == "available" && !model.loadable {
        "ERROR".to_owned()
    } else {
        model.state.to_ascii_uppercase()
    }
}

fn type_badge(model: &proto::LocalModelInfo) -> Line<'static> {
    let value = if model.library.is_empty() {
        "UNKNOWN"
    } else {
        &model.library
    };
    Line::from(Span::styled(
        format!(" {value} "),
        Style::new().fg(theme::GLACIER).bg(theme::RAISED),
    ))
}

fn features(model: &proto::LocalModelInfo) -> Line<'static> {
    let mut spans = Vec::new();
    for (enabled, label, color) in [
        (model.tool_use, "⌘ tools", theme::SIGNAL),
        (model.thinking, "◈ think", theme::GLACIER),
        (model.vision, "◉ vision", theme::SUCCESS),
    ] {
        if enabled {
            spans
                .push(Span::styled(format!(" {label} "), Style::new().fg(color).bg(theme::RAISED)));
            spans.push(Span::raw(" "));
        }
    }
    if spans.is_empty() {
        spans.push(Span::styled("—", Style::new().fg(theme::MUTED)));
    }
    Line::from(spans)
}

fn actions(state: &str, managed: bool) -> Line<'static> {
    let text = match state {
        "READY" => "■ unload",
        "AVAILABLE" if managed => "▶ load  × remove",
        "AVAILABLE" => "▶ load",
        "ERROR" if managed => "× remove",
        "PAUSED" | "PARTIAL" | "FAILED" => "↻ resume  × remove",
        "LOADING" | "UNLOADING" | "REMOVING" | "◌ WORKING" => "◌ working  ■ stop",
        _ => "—",
    };
    Line::from(Span::styled(text, Style::new().fg(theme::GLACIER)))
}

fn state_color(state: &str) -> Color {
    match state {
        "READY" => theme::SUCCESS,
        "AVAILABLE" => theme::SIGNAL,
        "ERROR" => theme::DANGER,
        "REMOVING" => theme::MUTED,
        _ => theme::GLACIER,
    }
}
