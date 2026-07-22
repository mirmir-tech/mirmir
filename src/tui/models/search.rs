use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Row, Table, TableState},
};

use super::{
    super::{app::App, theme},
    bytes,
};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let title = title(app);
    let rows = app.visible_catalog_models().map(|model| {
        Row::new(vec![
            Cell::from(if model.gated {
                format!("🔒 {}", model.id)
            } else {
                model.id.clone()
            }),
            Cell::from(format_label(model)),
            Cell::from(model.estimated_required_bytes.map_or_else(|| "—".to_owned(), bytes)),
            Cell::from(features(model.tool_use, model.thinking, model.vision)),
            Cell::from(status(model)),
            Cell::from(if is_local(model) {
                "✓ downloaded"
            } else {
                "↓ download"
            }),
        ])
    });
    let header = Row::new(["NAME", "FORMAT", "SIZE", "FEATURES", "FIT", "ACTION"])
        .style(Style::new().fg(theme::MUTED).add_modifier(Modifier::BOLD));
    let widths = [
        Constraint::Percentage(34),
        Constraint::Length(18),
        Constraint::Length(11),
        Constraint::Percentage(23),
        Constraint::Length(13),
        Constraint::Length(14),
    ];
    let selected = (app.visible_catalog_count() > 0).then_some(app.catalog_selected);
    let mut state = TableState::default().with_selected(selected);
    frame.render_stateful_widget(
        Table::new(rows, widths)
            .header(header)
            .row_highlight_style(Style::new().bg(theme::RAISED).fg(theme::INK))
            .block(
                Block::bordered()
                    .title(title)
                    .border_style(Style::new().fg(theme::GLACIER))
                    .style(Style::new().bg(theme::SURFACE)),
            ),
        area,
        &mut state,
    );
}

fn format_label(model: &crate::rpc::proto::CatalogModel) -> String {
    let value = if model.encoding.is_empty() || model.encoding == "Unknown" {
        &model.ecosystem
    } else {
        &model.encoding
    };
    value.to_ascii_uppercase()
}

fn title(app: &App) -> String {
    if app.catalog_loading() {
        let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
            [usize::try_from(app.animation_tick % 10).unwrap_or(0)];
        format!(" {spinner} SEARCHING HUGGING FACE ")
    } else if let Some(error) = &app.catalog_error {
        format!(" SEARCH FAILED — {error} ")
    } else {
        " HUGGING FACE RESULTS ".to_owned()
    }
}

fn status(model: &crate::rpc::proto::CatalogModel) -> Line<'static> {
    let (value, color) = if is_local(model) {
        ("DOWNLOADED", theme::SUCCESS)
    } else {
        match model.memory_fit.as_str() {
            "fits" => ("FITS", theme::SUCCESS),
            "tight" => ("TIGHT", theme::SIGNAL),
            "does_not_fit" => ("NO FIT", theme::DANGER),
            _ => ("UNKNOWN", theme::MUTED),
        }
    };
    Line::from(Span::styled(format!(" {value} "), Style::new().fg(color).bg(theme::RAISED)))
}

fn is_local(model: &crate::rpc::proto::CatalogModel) -> bool {
    model.downloaded || matches!(model.local_source.as_str(), "mirmir" | "hf_cache")
}

fn features(tools: bool, thinking: bool, vision: bool) -> Line<'static> {
    let labels = [(tools, "⌘ tools"), (thinking, "◈ think"), (vision, "◉ vision")];
    let value = labels
        .into_iter()
        .filter(|(enabled, _)| *enabled)
        .map(|(_, label)| label)
        .collect::<Vec<_>>()
        .join("  ");
    Line::from(if value.is_empty() {
        "—".to_owned()
    } else {
        value
    })
}
