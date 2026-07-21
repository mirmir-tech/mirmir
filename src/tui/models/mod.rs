mod local;
mod search;
mod transfer;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    widgets::{Block, Clear, Paragraph},
};

use super::{app::App, theme};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) -> Rect {
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(if app.transfer_repo.is_some() {
            4
        } else {
            2
        }),
    ])
    .split(area);
    search_field(frame, rows[0], app);
    local::draw(frame, rows[1], app);
    transfer::draw(frame, rows[2], app);
    if app.editing_search {
        frame.render_widget(Clear, rows[1]);
        search::draw(frame, rows[1], app);
    }
    rows[1]
}

fn search_field(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let (text, color) = if app.editing_search {
        let query = format!("{}▌", app.search_query);
        let width = usize::from(area.width.saturating_sub(4));
        (format!("{query:<width$}×"), theme::GLACIER)
    } else {
        ("/  Search Hugging Face".to_owned(), theme::MUTED)
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::bordered().border_style(Style::new().fg(color)))
            .style(Style::new().fg(theme::INK).bg(theme::SURFACE)),
        area,
    );
}

pub(super) fn bytes(value: u64) -> String {
    const GIB: u64 = 1_073_741_824;
    format!("{}.{:02} GiB", value / GIB, value % GIB * 100 / GIB)
}
