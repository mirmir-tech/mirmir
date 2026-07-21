mod content;
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
    let composer_height = if app.chat_image.is_some() {
        7
    } else {
        6
    };
    let rows =
        Layout::vertical([Constraint::Min(8), Constraint::Length(composer_height)]).split(area);
    conversation(frame, rows[0], app);
    composer(frame, rows[1], app);
}

fn conversation(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = Vec::new();
    for (index, message) in app.chat_messages.iter().enumerate() {
        let (label, color) = if message.role == "user" {
            ("YOU", theme::GLACIER)
        } else {
            ("MIRMIR", theme::SIGNAL)
        };
        lines.push(Line::from(vec![
            Span::styled("╭ ", Style::new().fg(color)),
            Span::styled(label, Style::new().fg(color).add_modifier(Modifier::BOLD)),
        ]));
        reasoning(&mut lines, app, index, &message.thought);
        for mut line in content::message_lines(&message.content) {
            line.spans.insert(0, Span::styled("│ ", Style::new().fg(color)));
            lines.push(line);
        }
        lines.push(Line::from(Span::styled("╰", Style::new().fg(color))));
        lines.push(Line::default());
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "Load a model and start a conversation below.",
            Style::new().fg(theme::MUTED),
        )));
    }
    render_scrolled(frame, area, app.chat_scroll, lines);
}

fn reasoning(lines: &mut Vec<Line<'static>>, app: &App, index: usize, thought: &str) {
    let active = index + 1 == app.chat_messages.len() && app.chat_status == ChatStatus::Generating;
    if thought.is_empty() && !active {
        return;
    }
    let dots = if active {
        ".".repeat(usize::try_from(app.animation_tick / 20 % 3 + 1).unwrap_or(3))
    } else {
        String::new()
    };
    let marker = if app.chat_reasoning.expanded() {
        "▼"
    } else {
        "▶"
    };
    lines.push(Line::from(Span::styled(
        format!("│ {marker} Thinking{dots}"),
        Style::new().fg(theme::THOUGHT).add_modifier(Modifier::ITALIC),
    )));
    if app.chat_reasoning.expanded() {
        for line in content::thought_lines(thought) {
            let mut spans = vec![Span::styled("│   ", Style::new().fg(theme::THOUGHT))];
            spans.extend(line.spans);
            lines.push(Line::from(spans));
        }
    }
}

fn render_scrolled(
    frame: &mut Frame<'_>,
    area: Rect,
    from_bottom: usize,
    lines: Vec<Line<'static>>,
) {
    let content_height = rendered_height(&lines, area.width.saturating_sub(2));
    let viewport_height = usize::from(area.height.saturating_sub(2));
    let max_scroll = content_height.saturating_sub(viewport_height);
    let offset = max_scroll.saturating_sub(from_bottom.min(max_scroll));
    let paragraph = Paragraph::new(Text::from(lines))
        .block(Block::bordered().title(" CHAT ").border_style(Style::new().fg(theme::BORDER)))
        .style(Style::new().fg(theme::INK).bg(theme::SURFACE))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph.scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)), area);
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

fn composer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let locked = app.chat_status == ChatStatus::Generating;
    let mut lines = Vec::new();
    if let Some(image) = &app.chat_image {
        lines.push(Line::from(vec![
            Span::styled("▣ ", Style::new().fg(theme::SUCCESS)),
            Span::styled(&image.name, Style::new().fg(theme::INK)),
            Span::styled("  Ctrl+D ×", Style::new().fg(theme::MUTED)),
        ]));
    }
    let input = if locked {
        "generation in progress — input locked…".to_owned()
    } else if app.chat_input.is_empty() {
        "Type a message…▌".to_owned()
    } else {
        format!("{}▌", app.chat_input)
    };
    lines.push(Line::from(input));
    lines.push(Line::default());
    lines.push(composer_footer(app));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(" MESSAGE ").border_style(Style::new().fg(if locked {
                theme::MUTED
            } else {
                theme::GLACIER
            })))
            .style(
                Style::new()
                    .fg(if locked {
                        theme::MUTED
                    } else {
                        theme::INK
                    })
                    .bg(theme::SURFACE),
            ),
        area,
    );
}

fn composer_footer(app: &App) -> Line<'static> {
    let model = app.selected_chat_model().map_or("no model", |model| model.id.as_str());
    let parameters = app
        .chat_parameters
        .as_ref()
        .filter(|parameters| parameters.model == model)
        .map_or("", |_| " · ONE-OFF PARAMETERS");
    let (ttft, prefill, decode, tokens) = app.chat_live_metrics.as_ref().map_or_else(
        || {
            app.chat_metrics.as_ref().map_or_else(
                || ("—".to_owned(), "—".to_owned(), "—".to_owned(), "—".to_owned()),
                |m| {
                    (
                        ms(m.ttft_ms),
                        rate(m.prefill_tokens_per_second),
                        rate(m.decode_tokens_per_second),
                        format!("{}p / {}c", m.prompt_tokens, m.completion_tokens),
                    )
                },
            )
        },
        |m| {
            (
                ms(m.ttft_ms.or(m.ttft_pending_ms)),
                rate(m.prefill_tokens_per_second),
                rate(m.decode_tokens_per_second),
                format!("{}p / {}c", m.prompt_tokens, m.completion_tokens),
            )
        },
    );
    Line::from(vec![
        Span::styled(
            format!("＋ attach   {model}{parameters}"),
            Style::new().fg(theme::GLACIER).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   TTFT {ttft} · prefill {prefill} · decode {decode} · {tokens}"),
            Style::new().fg(theme::MUTED),
        ),
        Span::styled("   ↑ Enter", Style::new().fg(theme::SUCCESS).add_modifier(Modifier::BOLD)),
    ])
}

fn rate(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2} tok/s"))
}

fn ms(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2} ms"))
}
