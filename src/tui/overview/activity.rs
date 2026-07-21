use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};

use crate::{
    rpc::proto,
    tui::{app::App, theme},
};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) -> Rect {
    let columns =
        Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)]).split(area);
    let items = app.activities.iter().map(item).collect::<Vec<_>>();
    let selected =
        (!items.is_empty()).then_some(app.activity_selected.min(items.len().saturating_sub(1)));
    let mut state = ListState::default().with_selected(selected);
    let title = app.activity_error.as_ref().map_or_else(
        || " RECENT ACTIVITY ".to_owned(),
        |error| format!(" RECENT ACTIVITY — {error} "),
    );
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::bordered().title(title).border_style(Style::new().fg(theme::BORDER)))
            .highlight_style(Style::new().bg(theme::RAISED).fg(theme::INK)),
        columns[0],
        &mut state,
    );
    runtime(frame, columns[1], app);
    columns[0]
}

fn item(event: &proto::ActivityEvent) -> ListItem<'static> {
    let (symbol, color) = match event.state.as_str() {
        "completed" => ("●", theme::SUCCESS),
        "failed" => ("●", theme::DANGER),
        "cancelled" => ("■", theme::MUTED),
        "cancelling" => ("◐", theme::SIGNAL),
        _ => ("◌", theme::GLACIER),
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{symbol} "), Style::new().fg(color)),
        Span::styled(
            event.kind.to_uppercase(),
            Style::new().fg(theme::INK).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {}  {}", event.target, event.stage), Style::new().fg(theme::MUTED)),
    ]))
}

fn runtime(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let telemetry = app.telemetry.as_ref();
    let requests = telemetry.map_or_else(
        || "—".to_owned(),
        |t| format!("{}/{} ok", t.completed_requests, t.total_requests),
    );
    let stage = telemetry.map_or("idle", |t| {
        if t.active_requests == 0 {
            "idle"
        } else {
            t.active_stage.as_str()
        }
    });
    let lines = vec![
        field("SERVER", &app.server_version),
        field("MODELS", &app.models.len().to_string()),
        field("REQUESTS", &requests),
        field("RUNTIME", stage),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" RUNTIME ")
                .border_style(Style::new().fg(theme::BORDER)),
        ),
        area,
    );
}

fn field(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<10}"), Style::new().fg(theme::MUTED)),
        Span::styled(value.to_owned(), Style::new().fg(theme::INK)),
    ])
}
