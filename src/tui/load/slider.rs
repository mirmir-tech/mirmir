use std::fmt::Write as _;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use super::{
    super::{
        app::{ContextChoice, ContextControl, LoadDialog, LoadRow, LoadStatus},
        ranges::{bar, grouped},
        theme,
    },
    capabilities,
};

pub fn draw(frame: &mut Frame<'_>, area: Rect, dialog: &LoadDialog) {
    // Embedding and reranking models show what the checkpoint does; one with
    // a K/V cache, such as a causal reranker, keeps its context row below.
    let rows = u16::from(dialog.context.is_some());
    let [summary, area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(rows)]).areas(area);
    if !capabilities::draw(frame, summary, dialog) {
        draw_rows(frame, summary.union(area), dialog);
    } else if rows > 0 {
        draw_rows(frame, area, dialog);
    }
}

fn draw_rows(frame: &mut Frame<'_>, area: Rect, dialog: &LoadDialog) {
    let ranges = dialog.ranges();
    let items = dialog.rows().map(|row| {
        let (label, bar, value) = match row {
            LoadRow::Context => ("context", context_bar(dialog), context_value(dialog)),
            LoadRow::Field(index) => {
                let range = ranges[index];
                (range.label, range.bar(&dialog.fields[index]), dialog.fields[index].clone())
            },
        };
        ListItem::new(Line::from(vec![
            Span::styled(format!("{label:<20}"), Style::new().fg(theme::MUTED)),
            Span::styled(format!("{bar} "), Style::new().fg(theme::GLACIER)),
            Span::styled(value, Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
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

fn context_bar(dialog: &LoadDialog) -> String {
    bar(dialog.context.as_ref().map_or(0.0, ContextControl::ratio))
}

fn context_value(dialog: &LoadDialog) -> String {
    let Some(context) = dialog.context.as_ref() else {
        return String::new();
    };
    let mut value = match context.choice {
        ContextChoice::Automatic => {
            format!("Automatic · {} now", grouped(context.limits.automatic_tokens))
        },
        ContextChoice::Tokens(tokens) => format!("{} tokens", grouped(tokens)),
    };
    if let Some(resident) = context.limits.resident_tokens {
        let _ignored = write!(value, " · loaded with {}", grouped(resident));
    }
    value
}
