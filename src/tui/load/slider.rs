use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use super::{
    super::{
        app::{LoadDialog, LoadStatus},
        theme,
    },
    capabilities,
};

const LABELS: [&str; 5] = ["max tokens", "temperature", "top p", "top k", "repetition penalty"];
const RANGES: [(f64, f64); 5] =
    [(1.0, 131_072.0), (0.0, 2.0), (0.0, 1.0), (0.0, 200.0), (0.5, 2.0)];

pub fn draw(frame: &mut Frame<'_>, area: Rect, dialog: &LoadDialog) {
    if capabilities::draw(frame, area, dialog) {
        return;
    }
    let items = LABELS.iter().zip(&dialog.fields).enumerate().map(|(index, (label, value))| {
        ListItem::new(Line::from(vec![
            Span::styled(format!("{label:<20}"), Style::new().fg(theme::MUTED)),
            Span::styled(
                display(index, value),
                Style::new().fg(theme::INK).add_modifier(Modifier::BOLD),
            ),
        ]))
    });
    let selected = (dialog.status == LoadStatus::Editing).then_some(dialog.selected);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items).highlight_style(Style::new().fg(theme::GLACIER).bg(theme::RAISED)),
        area,
        &mut state,
    );
}

fn display(index: usize, value: &str) -> String {
    let (minimum, maximum) = RANGES[index];
    let current = value.parse::<f64>().unwrap_or(minimum).clamp(minimum, maximum);
    let filled = format!("{:.0}", ((current - minimum) / (maximum - minimum)) * 12.0)
        .parse::<usize>()
        .unwrap_or(0)
        .min(12);
    format!("{value:<10} {}{}", "━".repeat(filled), "─".repeat(12 - filled))
}
