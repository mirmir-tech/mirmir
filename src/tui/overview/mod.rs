mod activity;
mod cards;
mod charts;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use super::app::App;

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) -> Rect {
    let rows = Layout::vertical([Constraint::Length(7), Constraint::Length(9), Constraint::Min(5)])
        .split(area);
    cards::draw(frame, rows[0], app);
    charts::draw(frame, rows[1], app);
    activity::draw(frame, rows[2], app)
}

pub(super) fn format_rate(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.1} tok/s"))
}

pub(super) fn format_ms(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.1} ms"))
}
