use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Paragraph, Row, Table, TableState, Wrap},
};

use super::{
    app::{App, ConfigurationTarget, ConfigurationView},
    theme,
};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) -> Rect {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(8), Constraint::Length(3)])
        .split(area);
    paths(frame, rows[0], app);
    if app.configuration_view == ConfigurationView::Raw {
        raw(frame, rows[1], app);
    } else {
        values(frame, rows[1], app);
    }
    action(frame, rows[2], app);
    rows[1]
}

fn paths(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = app.configuration.as_ref().map_or_else(
        || "waiting for configuration snapshot".to_owned(),
        |config| {
            format!(
                "config {}   ·   secrets {}   ·   raw TOML available",
                config.config_path, config.secrets_path
            )
        },
    );
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::bordered().border_style(Style::new().fg(theme::BORDER)))
            .style(Style::new().fg(theme::MUTED).bg(theme::SURFACE)),
        area,
    );
}

fn values(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = table_rows(app);
    let header = Row::new(["SETTING", "EFFECTIVE VALUE", "SOURCE", "RESTART", "ACTION"])
        .style(Style::new().fg(theme::MUTED).add_modifier(Modifier::BOLD));
    let widths = [
        Constraint::Percentage(35),
        Constraint::Percentage(28),
        Constraint::Length(14),
        Constraint::Length(10),
        Constraint::Length(16),
    ];
    let selected = (!rows.is_empty()).then_some(app.configuration_selected);
    let mut state = TableState::default().with_selected(selected);
    frame.render_stateful_widget(
        Table::new(rows, widths)
            .header(header)
            .row_highlight_style(Style::new().bg(theme::RAISED).fg(theme::INK))
            .block(
                Block::bordered()
                    .title(" SETTINGS ")
                    .border_style(Style::new().fg(theme::GLACIER)),
            ),
        area,
        &mut state,
    );
}

fn table_rows(app: &App) -> Vec<Row<'static>> {
    let mut rows = Vec::with_capacity(app.configuration_count());
    let hf = app.configuration.as_ref().and_then(|config| config.hugging_face_token.as_ref());
    let http = app.configuration.as_ref().and_then(|config| config.http_api_key.as_ref());
    rows.push(secret_row("hugging_face.token", hf, "edit · test · remove"));
    rows.push(secret_row("server.api_key", http, "edit · remove"));
    if let Some(config) = &app.configuration {
        rows.extend(config.values.iter().map(|value| {
            Row::new(vec![
                Cell::from(value.key.clone()),
                Cell::from(value.value.clone()),
                Cell::from(value.source.clone()),
                Cell::from(if value.restart_required {
                    "required"
                } else {
                    "live"
                }),
                Cell::from(if value.editable {
                    "edit"
                } else {
                    "read only"
                }),
            ])
        }));
    }
    rows
}

fn secret_row(
    key: &str,
    state: Option<&crate::rpc::proto::SecretState>,
    action: &str,
) -> Row<'static> {
    let configured = state.is_some_and(|state| state.configured);
    Row::new(vec![
        Cell::from(key.to_owned()),
        Cell::from(if configured {
            "********"
        } else {
            "not configured"
        }),
        Cell::from(state.map_or("unknown", |state| state.source.as_str()).to_owned()),
        Cell::from("live"),
        Cell::from(action.to_owned()),
    ])
}

fn raw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = app.configuration.as_ref().map_or("", |config| config.raw_toml.as_str());
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(
                Block::bordered()
                    .title(" RAW TOML ")
                    .border_style(Style::new().fg(theme::GLACIER)),
            )
            .style(Style::new().fg(theme::INK).bg(theme::SURFACE)),
        area,
    );
}

fn action(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = app.configuration_edit.as_ref().map_or_else(
        || {
            app.configuration_message
                .clone()
                .unwrap_or_else(|| "Select a setting to inspect or change it".to_owned())
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
        Paragraph::new(Line::from(vec![Span::styled(
            text,
            Style::new().fg(theme::INK).add_modifier(Modifier::BOLD),
        )]))
        .block(Block::bordered().border_style(Style::new().fg(theme::SIGNAL))),
        area,
    );
}
