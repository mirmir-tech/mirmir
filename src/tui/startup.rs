use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use super::theme;

pub fn draw(frame: &mut Frame<'_>, animation_tick: u64) {
    let area = centered(frame.area(), 50, 7);
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        [usize::try_from(animation_tick % 10).unwrap_or(0)];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::default(),
            Line::from(vec![
                Span::styled(
                    format!("{spinner} "),
                    Style::new().fg(theme::GLACIER).add_modifier(Modifier::BOLD),
                ),
                Span::styled("Connecting to runtime", Style::new().fg(theme::INK)),
            ]),
            Line::from(Span::styled(
                "Loading models, telemetry and settings…",
                Style::new().fg(theme::MUTED),
            )),
        ])
        .alignment(Alignment::Center)
        .block(Block::bordered().border_style(Style::new().fg(theme::BORDER)))
        .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

fn centered(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = area.width.saturating_sub(4).min(max_width);
    let height = area.height.saturating_sub(2).min(max_height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
