use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};

use super::{app::App, theme};
use crate::rpc::proto;

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) -> Rect {
    let columns =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).split(area);
    let items = app.activities.iter().map(activity_item).collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(
        (!items.is_empty()).then_some(app.activity_selected.min(items.len().saturating_sub(1))),
    );
    let title = app
        .activity_error
        .as_ref()
        .map_or_else(|| " ACTIVITY ".to_owned(), |error| format!(" ACTIVITY — {error} "));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::bordered().title(title).border_style(Style::new().fg(theme::BORDER)))
            .highlight_style(Style::new().bg(theme::RAISED).fg(theme::INK)),
        columns[0],
        &mut state,
    );
    details(frame, columns[1], app.activities.get(app.activity_selected));
    columns[0]
}

fn activity_item(event: &proto::ActivityEvent) -> ListItem<'static> {
    let (symbol, color) = state_style(&event.state);
    let progress = match (event.current, event.total) {
        (Some(current), Some(total)) if total > 0 => format!("  {current}/{total}"),
        _ => String::new(),
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{symbol} "), Style::new().fg(color)),
        Span::styled(event.kind.to_uppercase(), Style::new().fg(theme::INK)),
        Span::styled(format!("  {}", event.target), Style::new().fg(theme::GLACIER)),
        Span::styled(format!("  {}{progress}", event.stage), Style::new().fg(theme::MUTED)),
    ]))
}

fn details(frame: &mut Frame<'_>, area: Rect, event: Option<&proto::ActivityEvent>) {
    let lines = event.map_or_else(
        || vec![Line::from(Span::styled("No operations yet", Style::new().fg(theme::MUTED)))],
        |event| {
            vec![
                field("ID", &event.operation_id),
                field("TYPE", &event.kind),
                field("TARGET", &event.target),
                field("STATE", &event.state),
                field("STAGE", &event.stage),
                field("DETAIL", &event.detail),
                field(
                    "CANCELLABLE",
                    if event.cancellable {
                        "yes"
                    } else {
                        "no"
                    },
                ),
            ]
        },
    );
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" OPERATION ")
                .border_style(Style::new().fg(theme::BORDER)),
        ),
        area,
    );
}

fn field(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<12}"), Style::new().fg(theme::MUTED)),
        Span::styled(value.to_owned(), Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
    ])
}

fn state_style(state: &str) -> (&'static str, ratatui::style::Color) {
    match state {
        "completed" => ("●", theme::SUCCESS),
        "failed" => ("●", theme::DANGER),
        "cancelled" => ("■", theme::MUTED),
        "cancelling" => ("◐", theme::SIGNAL),
        _ => ("◌", theme::GLACIER),
    }
}
