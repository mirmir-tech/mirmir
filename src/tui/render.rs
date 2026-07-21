use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::{
    app::{App, Screen, WORKSPACE_PREFIX, WORKSPACE_TABS},
    chat, configuration, confirm, help, load, models, overview, theme,
};

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    frame.render_widget(Block::new().style(Style::new().bg(theme::CANVAS)), frame.area());
    let vertical = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(12),
        Constraint::Length(1),
    ])
    .split(frame.area());
    header(frame, vertical[0], app);
    app.set_workspace_area(vertical[1]);
    workspace(frame, vertical[1], app);
    let list_area = match app.screen {
        Screen::Dashboard => Some(overview::draw(frame, vertical[2], app)),
        Screen::Models => Some(models::draw(frame, vertical[2], app)),
        Screen::Chat => {
            chat::draw(frame, vertical[2], app);
            None
        },
        Screen::Settings => Some(configuration::draw(frame, vertical[2], app)),
    };
    app.set_list_view(list_area);
    if app.load_dialog.is_some() {
        load::draw(frame, app);
    }
    if app.chat_settings_dialog.is_some() {
        chat::settings::draw(frame, app);
    }
    if let Some(dialog) = app.remove_dialog.as_ref() {
        confirm::draw_remove(frame, dialog);
    }
    if app.navigation.exit_dialog {
        confirm::draw_exit(frame);
    }
    if app.help_open {
        help::draw(frame, app.screen);
    }
    footer(frame, vertical[3], app);
}

fn header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let status = if app.last_error.is_some() {
        "DEGRADED"
    } else {
        "HEALTHY"
    };
    let status_color = if app.last_error.is_some() {
        theme::DANGER
    } else {
        theme::SUCCESS
    };
    let line = Line::from(vec![
        Span::styled("  MiRMiR", Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
        Span::styled("  /  LOCAL RUNTIME", Style::new().fg(theme::MUTED)),
        Span::raw("    "),
        Span::styled(status, Style::new().fg(status_color).add_modifier(Modifier::BOLD)),
    ]);
    frame.render_widget(
        Paragraph::new(line)
            .block(
                Block::new()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::new().fg(theme::BORDER)),
            )
            .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

fn workspace(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut spans = vec![Span::styled(WORKSPACE_PREFIX, Style::new().fg(theme::MUTED))];
    for (screen, shortcut, name, _) in WORKSPACE_TABS {
        let selected = app.screen == screen;
        let style = if selected {
            Style::new().fg(theme::GLACIER).bg(theme::RAISED).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme::MUTED)
        };
        let marker = if selected {
            "▌"
        } else {
            " "
        };
        spans.push(Span::styled(format!(" {marker}[F{shortcut}] {name} "), style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .block(Block::bordered().border_style(Style::new().fg(theme::BORDER)))
            .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

fn footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let ownership = if app.server_reused {
        "ATTACHED"
    } else {
        "LOCAL OWNER"
    };
    let text = format!(
        " v{} · gRPC {}  ·  ? help  ·  Esc close  ·  q quit  ·  {ownership}",
        app.server_version, app.protocol_version,
    );
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Right)
            .style(Style::new().fg(theme::MUTED).bg(theme::CANVAS)),
        area,
    );
}
