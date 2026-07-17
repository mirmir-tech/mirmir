mod local;
mod transfer;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};

use super::{app::App, theme};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) -> Rect {
    let transfer_height = if app.transfer_repo.is_some() {
        4
    } else {
        2
    };
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Length(transfer_height),
        Constraint::Min(8),
        Constraint::Length(6),
    ])
    .split(area);
    search(frame, rows[0], app);
    memory(frame, rows[1], app);
    transfer::draw(frame, rows[2], app);
    if app.searching_models() {
        results(frame, rows[3], app);
    } else {
        local::draw(frame, rows[3], app);
    }
    details(frame, rows[4], app);
    rows[3]
}

fn search(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let cursor = if app.editing_search {
        "▌"
    } else {
        ""
    };
    let query = if app.search_query.is_empty() && !app.editing_search {
        "Search Hugging Face".to_owned()
    } else {
        format!("{}{cursor}", app.search_query)
    };
    let color = if app.editing_search {
        theme::GLACIER
    } else {
        theme::MUTED
    };
    frame.render_widget(
        Paragraph::new(query)
            .block(Block::bordered().title(" HF SEARCH ").border_style(Style::new().fg(color)))
            .style(Style::new().fg(theme::INK).bg(theme::SURFACE)),
        area,
    );
}

fn memory(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let total = app.total_memory_bytes.map_or_else(|| "unknown".to_owned(), bytes);
    let available = app.available_memory_bytes.map_or_else(|| "unknown".to_owned(), bytes);
    let line = format!(
        " memory: {available} available / {total} total  ·  local models: {}  ·  source: {}",
        app.local_model_count(),
        app.memory_source
    );
    frame.render_widget(Paragraph::new(line).style(Style::new().fg(theme::MUTED)), area);
}

fn results(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app.visible_catalog_models().map(|model| {
        ListItem::new(Line::from(vec![
            Span::styled(
                format!(" {:<12} ", model.memory_fit),
                Style::new().fg(fit_color(&model.memory_fit)),
            ),
            Span::styled(
                local_badge(&model.local_source),
                Style::new().fg(local_color(&model.local_source)),
            ),
            Span::styled(gated_badge(model.gated), Style::new().fg(Color::Rgb(0xE8, 0xB1, 0x5A))),
            Span::styled(&model.id, Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("  {}", bytes_opt(model.estimated_required_bytes)),
                Style::new().fg(theme::MUTED),
            ),
        ]))
    });
    let selected = (app.visible_catalog_count() > 0).then_some(app.catalog_selected);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::bordered()
                    .title(" MODELS — SEARCH RESULTS ")
                    .border_style(Style::new().fg(theme::GLACIER)),
            )
            .highlight_style(Style::new().bg(theme::RAISED).fg(theme::GLACIER)),
        area,
        &mut state,
    );
}

fn details(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = if app.searching_models() {
        app.catalog_error.as_ref().map_or_else(
            || selected_details(app),
            |error| Line::from(Span::styled(error, Style::new().fg(theme::DANGER))),
        )
    } else {
        local::details(app)
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::bordered()
                    .title(" INSPECTOR ")
                    .border_style(Style::new().fg(theme::BORDER)),
            )
            .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

fn selected_details(app: &App) -> Line<'_> {
    app.selected_catalog_model().map_or_else(
        || {
            Line::from(Span::styled(
                "Ranking: supported+fits, tight, unknown, does_not_fit, incompatible.",
                Style::new().fg(theme::MUTED),
            ))
        },
        |model| {
            Line::from(vec![
                Span::styled(&model.architecture, Style::new().fg(theme::GLACIER)),
                Span::raw("  ·  "),
                Span::styled(
                    &model.compatibility,
                    Style::new().fg(compatibility_color(&model.compatibility)),
                ),
                Span::raw(format!(
                    "  ·  downloads {}  ·  likes {}  ·  {}  ·  confidence {}\n",
                    model.downloads,
                    model.likes,
                    if model.gated {
                        "gated"
                    } else {
                        "public"
                    },
                    model.confidence
                )),
                Span::styled(&model.reason, Style::new().fg(theme::MUTED)),
            ])
        },
    )
}

fn fit_color(value: &str) -> Color {
    match value {
        "fits" => theme::SUCCESS,
        "tight" => Color::Rgb(0xE8, 0xB1, 0x5A),
        "does_not_fit" => theme::DANGER,
        _ => theme::MUTED,
    }
}

fn local_badge(source: &str) -> &'static str {
    match source {
        "mirmir" => "MIRMIR   ",
        "hf_cache" => "HF CACHE ",
        _ => "         ",
    }
}

const fn gated_badge(gated: bool) -> &'static str {
    if gated {
        "GATED "
    } else {
        ""
    }
}

fn local_color(source: &str) -> Color {
    if source == "mirmir" {
        theme::SUCCESS
    } else {
        theme::SIGNAL
    }
}

fn compatibility_color(value: &str) -> Color {
    if value == "supported" {
        theme::SUCCESS
    } else {
        theme::DANGER
    }
}

fn bytes_opt(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_owned(), bytes)
}

fn bytes(value: u64) -> String {
    const GIB: f64 = 1_073_741_824.0;
    let value = value.to_string().parse::<f64>().unwrap_or(0.0);
    format!("{:.2} GiB", value / GIB)
}
