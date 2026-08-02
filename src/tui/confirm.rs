use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use super::{app::RemoveDialog, theme};

pub fn draw_exit(frame: &mut Frame<'_>) {
    let area = centered(frame.area(), 54, 9);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::default(),
            Line::from(Span::styled(
                "Close Mirmir?",
                Style::new().fg(theme::INK).add_modifier(Modifier::BOLD),
            )),
            Line::default(),
            Line::from(Span::styled("Confirmation required", Style::new().fg(theme::MUTED))),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::bordered()
                .title(" CONFIRM EXIT ")
                .border_style(Style::new().fg(theme::DANGER)),
        )
        .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

pub fn draw_remove(frame: &mut Frame<'_>, dialog: &RemoveDialog) {
    let area = centered(frame.area(), 78, 13);
    frame.render_widget(Clear, area);
    let ownership = if dialog.source == "HF CACHE" {
        "This cache may also be used by other applications."
    } else {
        "This model is stored in the Mirmir-managed cache."
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::default(),
            Line::from(Span::styled(
                format!("Remove {}?", dialog.id),
                Style::new().fg(theme::INK).add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled("Source  ", Style::new().fg(theme::MUTED)),
                Span::styled(&dialog.source, Style::new().fg(theme::SIGNAL)),
            ]),
            Line::from(vec![
                Span::styled("Snapshot  ", Style::new().fg(theme::MUTED)),
                Span::styled(&dialog.path, Style::new().fg(theme::INK)),
            ]),
            Line::default(),
            Line::from(Span::styled(
                "Cached model files will be permanently deleted.",
                Style::new().fg(theme::DANGER).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(ownership, Style::new().fg(theme::MUTED))),
            Line::default(),
            Line::from(Span::styled("Confirmation required", Style::new().fg(theme::MUTED))),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::bordered()
                .title(" CONFIRM MODEL REMOVAL ")
                .border_style(Style::new().fg(theme::DANGER)),
        )
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
