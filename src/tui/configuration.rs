use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};

use super::{
    app::{App, ConfigurationTarget},
    theme,
};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) -> Rect {
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(8),
        Constraint::Length(4),
        Constraint::Length(3),
    ])
    .split(area);
    paths(frame, rows[0], app);
    values(frame, rows[1], app);
    details(frame, rows[2], app);
    action(frame, rows[3], app);
    rows[1]
}

fn paths(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = app.configuration.as_ref().map_or_else(
        || "waiting for configuration snapshot".to_owned(),
        |config| format!("config  {}\nsecrets {}", config.config_path, config.secrets_path),
    );
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::bordered().title(" CONFIGURATION FILES "))
            .style(Style::new().fg(theme::MUTED).bg(theme::SURFACE)),
        area,
    );
}

fn values(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = rows(app).into_iter().map(ListItem::new);
    let mut state = ListState::default().with_selected(Some(app.configuration_selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::bordered()
                    .title(" SETTINGS — PERSISTED VALUE / SOURCE ")
                    .border_style(Style::new().fg(theme::GLACIER)),
            )
            .highlight_style(Style::new().bg(theme::RAISED).fg(theme::GLACIER)),
        area,
        &mut state,
    );
}

fn rows(app: &App) -> Vec<Line<'_>> {
    let mut rows = Vec::with_capacity(app.configuration_count());
    let hf = app.configuration.as_ref().and_then(|config| config.hugging_face_token.as_ref());
    let http = app.configuration.as_ref().and_then(|config| config.http_api_key.as_ref());
    rows.push(secret_row("hugging_face.token", hf));
    rows.push(secret_row("server.api_key", http));
    if let Some(config) = &app.configuration {
        rows.extend(config.values.iter().map(|value| {
            let restart = if value.restart_required {
                "  RESTART"
            } else {
                ""
            };
            Line::from(vec![
                Span::styled(format!(" {:<36}", value.key), Style::new().fg(theme::INK)),
                Span::styled(format!(" {:<24}", value.value), Style::new().fg(theme::GLACIER)),
                Span::styled(
                    format!(" {:<14}{restart}", value.source),
                    Style::new().fg(theme::MUTED),
                ),
            ])
        }));
    }
    rows
}

fn secret_row<'a>(key: &'a str, state: Option<&'a crate::rpc::proto::SecretState>) -> Line<'a> {
    let configured = state.is_some_and(|state| state.configured);
    let value = if configured {
        "configured"
    } else {
        "not configured"
    };
    let source = state.map_or("unknown", |state| state.source.as_str());
    Line::from(vec![
        Span::styled(format!(" {key:<36}"), Style::new().fg(theme::INK)),
        Span::styled(
            format!(" {value:<24}"),
            Style::new().fg(if configured {
                theme::SUCCESS
            } else {
                theme::MUTED
            }),
        ),
        Span::styled(format!(" {source:<14}"), Style::new().fg(theme::MUTED)),
    ])
}

fn details(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let line = if app.configuration_selected == 0 {
        "HF token is write-only over RPC and never returned or displayed."
    } else if app.configuration_selected == 1 {
        "HTTP API key is write-only over RPC and never returned or displayed."
    } else {
        "Use `auto` for optional runtime fields. RESTART marks non-live settings."
    };
    frame.render_widget(
        Paragraph::new(line)
            .block(Block::bordered().title(" DETAILS "))
            .style(Style::new().fg(theme::MUTED).bg(theme::SURFACE)),
        area,
    );
}

fn action(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = app.configuration_edit.as_ref().map_or_else(
        || {
            app.configuration_message
                .clone()
                .unwrap_or_else(|| "Select a setting to inspect or edit it".to_owned())
        },
        |edit| {
            let input = if edit.secret() {
                "•".repeat(edit.input.chars().count())
            } else {
                edit.input.clone()
            };
            let label = match &edit.target {
                ConfigurationTarget::Value(key) => key.as_str(),
                ConfigurationTarget::HuggingFaceToken => "hugging_face.token",
                ConfigurationTarget::HttpApiKey => "server.api_key",
            };
            format!("{label}: {input}▌")
        },
    );
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::bordered().border_style(Style::new().fg(theme::SIGNAL)))
            .style(Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
        area,
    );
}
