use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph},
};

use super::{
    app::{App, ChatSettingsDialog, ChatSettingsStatus},
    theme,
};

const LABELS: [&str; 6] =
    ["max tokens", "temperature", "top p", "top k", "repetition penalty", "seed"];

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let Some(dialog) = app.chat_settings_dialog.as_ref() else {
        return;
    };
    let area = centered(frame.area(), 70, 19);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::bordered()
            .title(" CHAT PARAMETERS ")
            .border_style(Style::new().fg(theme::GLACIER))
            .style(Style::new().bg(theme::SURFACE)),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin { horizontal: 2, vertical: 1 });
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(7),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .split(inner);
    header(frame, rows[0], dialog);
    fields(frame, rows[1], dialog);
    save_default(frame, rows[2], dialog);
    error(frame, rows[3], dialog);
    hint(frame, rows[4], dialog);
}

fn header(frame: &mut Frame<'_>, area: Rect, dialog: &ChatSettingsDialog) {
    let source = match dialog.status {
        ChatSettingsStatus::Inspecting => "reading model defaults",
        ChatSettingsStatus::Saving => "saving Mirmir defaults",
        ChatSettingsStatus::Editing if dialog.persisted => "Mirmir model defaults",
        ChatSettingsStatus::Editing => "model defaults",
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(&dialog.model, Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  ·  {source}"), Style::new().fg(theme::MUTED)),
        ])),
        area,
    );
}

fn fields(frame: &mut Frame<'_>, area: Rect, dialog: &ChatSettingsDialog) {
    let items = LABELS.iter().zip(&dialog.fields).map(|(label, value)| {
        let shown = if *label == "seed" && value.is_empty() {
            "auto"
        } else {
            value
        };
        ListItem::new(Line::from(vec![
            Span::styled(format!("{label:<20}"), Style::new().fg(theme::MUTED)),
            Span::styled(shown, Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
        ]))
    });
    let selected = (dialog.status == ChatSettingsStatus::Editing).then_some(dialog.selected);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items).highlight_style(Style::new().fg(theme::GLACIER).bg(theme::RAISED)),
        area,
        &mut state,
    );
}

fn save_default(frame: &mut Frame<'_>, area: Rect, dialog: &ChatSettingsDialog) {
    let (marker, color) = if dialog.save_default {
        ("[x]", theme::SUCCESS)
    } else {
        ("[ ]", theme::MUTED)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(marker, Style::new().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(
                " Save as model default (seed remains one-off)",
                Style::new().fg(theme::INK),
            ),
        ])),
        area,
    );
}

fn error(frame: &mut Frame<'_>, area: Rect, dialog: &ChatSettingsDialog) {
    frame.render_widget(
        Paragraph::new(dialog.error.as_deref().unwrap_or_default())
            .style(Style::new().fg(theme::DANGER)),
        area,
    );
}

fn hint(frame: &mut Frame<'_>, area: Rect, dialog: &ChatSettingsDialog) {
    let text = match dialog.status {
        ChatSettingsStatus::Inspecting => "Reading parameters…  ·  x/Esc close",
        ChatSettingsStatus::Saving => "Saving defaults…  ·  x/Esc close",
        ChatSettingsStatus::Editing => {
            "↑/↓ field  ·  type to edit  ·  s save default  ·  Enter apply  ·  x/Esc close"
        },
    };
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(Style::new().fg(theme::MUTED)),
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
