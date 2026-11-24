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
use crate::rpc::proto::{CatalogModel, CatalogVariant};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let title = title(app);
    let rows = app.visible_catalog_models().map(|model| {
        let variant = app.chosen_variant(model);
        let downloaded = app.catalog_downloaded(model);
        Row::new(vec![
            Cell::from(name(model)),
            Cell::from(format_label(model, variant)),
            Cell::from(size(model, variant)),
            Cell::from(features(model.tool_use, model.thinking, model.vision)),
            Cell::from(status(app.catalog_fit(model), downloaded)),
            Cell::from(if downloaded {
                "✓ downloaded"
            } else {
                "↓ download"
            }),
        ])
    });
    let header = Row::new(["NAME", "FORMAT · ←/→", "SIZE", "FEATURES", "FIT", "ACTION"])
        .style(Style::new().fg(theme::MUTED).add_modifier(Modifier::BOLD));
    let widths = [
        Constraint::Percentage(34),
        Constraint::Length(20),
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

fn name(model: &CatalogModel) -> String {
    let lock = if model.gated {
        "🔒 "
    } else {
        ""
    };
    format!("{lock}{}", model.id)
}

/// What the download needs; `~` marks a size estimated before its files
/// are listed.
fn size(model: &CatalogModel, variant: Option<&CatalogVariant>) -> String {
    let (required, estimated) = variant
        .map_or((model.estimated_required_bytes, false), |variant| {
            (variant.required_bytes, variant.size_kind == "estimated")
        });
    required.map_or_else(
        || "—".to_owned(),
        |required| {
            format!(
                "{}{}",
                if estimated {
                    "~"
                } else {
                    ""
                },
                bytes(required)
            )
        },
    )
}

/// A GGUF repository's chosen quantization between the ←/→ marks that
/// change it, and its place among them; the encoding of another.
fn format_label(model: &CatalogModel, variant: Option<&CatalogVariant>) -> String {
    let Some(variant) = variant else {
        return model.encoding.as_deref().unwrap_or(&model.ecosystem).to_ascii_uppercase();
    };
    let count = model.variants.len();
    if count < 2 {
        return format!("GGUF {}", variant.name);
    }
    let place = model
        .variants
        .iter()
        .position(|option| option.name == variant.name)
        .unwrap_or(0);
    format!("‹{}› {}/{count}", variant.name, place + 1)
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

fn status(fit: &str, downloaded: bool) -> Line<'static> {
    let (value, color) = if downloaded {
        ("DOWNLOADED", theme::SUCCESS)
    } else {
        match fit {
            "fits" => ("FITS", theme::SUCCESS),
            "tight" => ("TIGHT", theme::SIGNAL),
            "does_not_fit" => ("NO FIT", theme::DANGER),
            _ => ("UNKNOWN", theme::MUTED),
        }
    };
    Line::from(Span::styled(format!(" {value} "), Style::new().fg(color).bg(theme::RAISED)))
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
