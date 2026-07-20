mod content;
mod metrics;
pub(super) mod settings;
#[cfg(test)]
mod tests;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};

use super::{
    app::{App, ChatStatus},
    theme,
};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(3),
        Constraint::Length(if app.chat_image.is_some() {
            4
        } else {
            3
        }),
    ])
    .split(area);
    model(frame, rows[0], app);
    conversation(frame, rows[1], app);
    metrics::draw(frame, rows[2], app);
    input(frame, rows[3], app);
}

fn model(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let selected = app.selected_chat_model().map_or("none loaded", |model| model.id.as_str());
    let generating = app.chat_status == ChatStatus::Generating;
    let status = if generating {
        "GENERATING"
    } else {
        "READY"
    };
    let status_color = if generating {
        theme::SIGNAL
    } else {
        theme::SUCCESS
    };
    let overrides = app
        .selected_chat_model()
        .filter(|model| app.chat_parameters.as_ref().is_some_and(|value| value.model == model.id))
        .map_or("", |_| "  ONE-OFF PARAMETERS");
    let image_status = app.selected_chat_model().map_or("", |model| {
        if model.image_input {
            "  IMAGE READY"
        } else {
            "  TEXT ONLY"
        }
    });
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(selected, Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(status, Style::new().fg(status_color).add_modifier(Modifier::BOLD)),
            Span::styled(image_status, Style::new().fg(theme::GLACIER)),
            Span::styled(overrides, Style::new().fg(theme::GLACIER)),
        ]))
        .block(Block::bordered().title(" MODEL ").border_style(Style::new().fg(theme::BORDER)))
        .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

fn conversation(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = Vec::new();
    for (index, message) in app.chat_messages.iter().enumerate() {
        let (label, color) = if message.role == "user" {
            ("YOU", theme::GLACIER)
        } else {
            ("MIRMIR", theme::SIGNAL)
        };
        lines.push(Line::from(Span::styled(
            label,
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        )));
        let thinking = message.role == "assistant"
            && message.content.is_empty()
            && message.thought.is_empty()
            && app.chat_status == ChatStatus::Generating
            && index + 1 == app.chat_messages.len();
        if thinking {
            let dots = ".".repeat(usize::try_from(app.animation_tick / 20 % 3 + 1).unwrap_or(3));
            lines.push(Line::from(Span::styled(
                format!("Thinking{dots}"),
                Style::new().fg(theme::THOUGHT).add_modifier(Modifier::ITALIC),
            )));
        } else {
            if !message.thought.is_empty() {
                lines.extend(content::thought_lines(&message.thought));
                lines.push(Line::default());
            }
            lines.extend(content::message_lines(&message.content));
        }
        lines.push(Line::default());
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "Load a model, type a message, and press Enter.",
            Style::new().fg(theme::MUTED),
        )));
    }
    let content_height = rendered_height(&lines, area.width.saturating_sub(2));
    let paragraph = Paragraph::new(Text::from(lines))
        .block(Block::bordered().title(" CHAT ").border_style(Style::new().fg(theme::BORDER)))
        .style(Style::new().fg(theme::INK).bg(theme::SURFACE))
        .wrap(Wrap { trim: false });
    let viewport_height = usize::from(area.height.saturating_sub(2));
    let max_scroll = content_height.saturating_sub(viewport_height);
    let from_bottom = app.chat_scroll.min(max_scroll);
    let offset = max_scroll.saturating_sub(from_bottom);
    let vertical = u16::try_from(offset).unwrap_or(u16::MAX);
    frame.render_widget(paragraph.scroll((vertical, 0)), area);
    if max_scroll > 0 {
        let mut state = ScrollbarState::new(content_height)
            .position(offset)
            .viewport_content_length(viewport_height);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin { vertical: 1, horizontal: 0 }),
            &mut state,
        );
    }
}

fn rendered_height(lines: &[Line<'_>], width: u16) -> usize {
    let width = usize::from(width.max(1));
    lines.iter().map(|line| line.width().max(1).div_ceil(width)).sum()
}

fn input(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let locked = app.chat_status == ChatStatus::Generating;
    let empty = app.chat_input.is_empty();
    let cursor = if locked {
        ""
    } else {
        "▌"
    };
    let placeholder = if locked {
        "generation in progress — input locked"
    } else if empty {
        "Type a message…"
    } else {
        &app.chat_input
    };
    let mut lines = Vec::new();
    if let Some(image) = &app.chat_image {
        lines.push(Line::from(vec![
            Span::styled("IMAGE  ", Style::new().fg(theme::SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(
                    "{} · {} KiB · Ctrl+D removes",
                    image.name,
                    image.bytes.len().div_ceil(1024)
                ),
                Style::new().fg(theme::MUTED),
            ),
        ]));
    }
    lines.push(Line::from(format!("{placeholder}{cursor}")));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(" MESSAGE ").border_style(Style::new().fg(if locked {
                theme::MUTED
            } else {
                theme::GLACIER
            })))
            .style(
                Style::new()
                    .fg(if locked || empty {
                        theme::MUTED
                    } else {
                        theme::INK
                    })
                    .bg(theme::SURFACE),
            ),
        area,
    );
}
