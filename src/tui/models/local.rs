use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
};

use super::super::{app::App, theme};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app.visible_local_models().map(|model| {
        ListItem::new(Line::from(vec![
            Span::styled(
                format!(" {:<9} ", model.state),
                Style::new().fg(state_color(&model.state)),
            ),
            Span::styled(&model.id, Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
        ]))
    });
    let count = app.local_model_count();
    let selected = (count > 0).then_some(app.local_selected);
    let mut state = ListState::default().with_selected(selected);
    let title = if count == 0 {
        " MODELS — LOCAL · none downloaded "
    } else {
        " MODELS — LOCAL "
    };
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::bordered().title(title).border_style(Style::new().fg(theme::GLACIER)))
            .highlight_style(Style::new().bg(theme::RAISED).fg(theme::GLACIER)),
        area,
        &mut state,
    );
}

pub fn details(app: &App) -> Line<'_> {
    app.selected_local_model().map_or_else(
        || {
            Line::from(Span::styled(
                "No downloaded models are available locally.",
                Style::new().fg(theme::MUTED),
            ))
        },
        |model| {
            let source = if model.repo_id.is_empty() {
                "local path"
            } else if !model.managed {
                "external HF cache"
            } else {
                model.repo_id.as_str()
            };
            Line::from(vec![
                Span::styled(&model.state, Style::new().fg(state_color(&model.state))),
                Span::raw("  ·  "),
                Span::styled(source, Style::new().fg(theme::GLACIER)),
                Span::raw(format!("  ·  revision {}\n", revision(model))),
                Span::styled(&model.path, Style::new().fg(theme::MUTED)),
            ])
        },
    )
}

fn revision(model: &crate::rpc::proto::LocalModelInfo) -> &str {
    if model.commit.is_empty() {
        if model.revision.is_empty() {
            "local"
        } else {
            &model.revision
        }
    } else {
        &model.commit
    }
}

fn state_color(state: &str) -> Color {
    match state {
        "ready" => theme::SUCCESS,
        "loading" => theme::GLACIER,
        "available" => theme::SIGNAL,
        "missing" => theme::DANGER,
        _ => theme::MUTED,
    }
}
