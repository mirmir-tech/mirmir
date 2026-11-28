use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Row, Table, TableState},
};

use super::{
    super::{
        app::{App, LocalRow, LocalRowState},
        theme,
    },
    bytes,
};
use crate::application::{ActivityKind, LocalModelState};

#[derive(Clone, Copy, PartialEq, Eq)]
enum DisplayState {
    Working,
    State(LocalRowState),
}

impl DisplayState {
    const fn label(self) -> &'static str {
        match self {
            Self::Working => "◌ WORKING",
            Self::State(state) => state.label(),
        }
    }

    const fn color(self) -> Color {
        match self {
            Self::State(LocalRowState::Server(LocalModelState::Ready)) => theme::SUCCESS,
            Self::State(LocalRowState::Server(LocalModelState::Available)) => theme::SIGNAL,
            Self::State(LocalRowState::Server(LocalModelState::Error)) => theme::DANGER,
            Self::State(LocalRowState::Removing) => theme::MUTED,
            _ => theme::GLACIER,
        }
    }

    const fn actions(self, managed: bool) -> &'static str {
        match self {
            Self::State(LocalRowState::Server(LocalModelState::Ready)) => "■ unload",
            Self::State(LocalRowState::Server(LocalModelState::Available)) if managed => {
                "▶ load  × remove"
            },
            Self::State(LocalRowState::Server(LocalModelState::Available)) => "▶ load",
            Self::State(LocalRowState::Server(LocalModelState::Error)) if managed => "× remove",
            Self::State(state) if state.is_resumable() => "↻ resume  × remove",
            Self::Working
            | Self::State(
                LocalRowState::Server(LocalModelState::Loading)
                | LocalRowState::Unloading
                | LocalRowState::Removing,
            ) => "◌ working  ■ stop",
            Self::State(_) => "—",
        }
    }
}

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app.visible_local_models().map(|model| {
        let state = model_state(app, model);
        let row = Row::new(vec![
            Cell::from(model.id.clone()),
            Cell::from(type_badge(model)),
            Cell::from(bytes(model.size_bytes)),
            Cell::from(features(model)),
            Cell::from(Line::from(Span::styled(state.label(), Style::new().fg(state.color())))),
            Cell::from(Line::from(Span::styled(
                state.actions(model.managed),
                Style::new().fg(theme::GLACIER),
            ))),
        ]);
        if state == DisplayState::State(LocalRowState::Removing) {
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

fn model_state(app: &App, model: &LocalRow) -> DisplayState {
    if app.activities.iter().any(|event| {
        (event.target == model.id || event.target == model.repo_id)
            && event.state.is_live()
            && matches!(event.kind, Some(ActivityKind::Load | ActivityKind::Pull))
    }) {
        return DisplayState::Working;
    }
    if model.state == LocalRowState::Server(LocalModelState::Available) && !model.loadable {
        DisplayState::State(LocalRowState::Server(LocalModelState::Error))
    } else {
        DisplayState::State(model.state)
    }
}

fn type_badge(model: &LocalRow) -> Line<'static> {
    let value = if let Some(encoding) = &model.encoding {
        encoding
    } else if !model.ecosystem.is_empty() {
        &model.ecosystem
    } else if model.library.is_empty() {
        "UNKNOWN"
    } else {
        &model.library
    };
    Line::from(Span::styled(
        format!(" {value} "),
        Style::new().fg(theme::GLACIER).bg(theme::RAISED),
    ))
}

fn features(model: &LocalRow) -> Line<'static> {
    let mut spans = Vec::new();
    for (enabled, label, color) in [
        (model.features.tool_use, "⌘ tools", theme::SIGNAL),
        (model.features.thinking, "◈ think", theme::GLACIER),
        (model.features.vision, "◉ vision", theme::SUCCESS),
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
