use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use super::{app::Screen, theme};

pub fn draw(frame: &mut Frame<'_>, screen: Screen) {
    let area = centered(frame.area(), 76, 22);
    frame.render_widget(Clear, area);
    let mut lines = vec![
        shortcut("Tab / Shift+Tab", "next / previous tab"),
        shortcut("F1 … F4", "open a tab directly"),
        shortcut("q", "quit outside the chat input"),
        shortcut("Ctrl+C", "quit immediately from any view"),
        shortcut("? / Esc", "close this help"),
        Line::default(),
    ];
    lines.extend(screen_help(screen));
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(
                Block::bordered()
                    .title(format!(" {} HELP ", title(screen)))
                    .border_style(Style::new().fg(theme::GLACIER)),
            )
            .style(Style::new().fg(theme::INK).bg(theme::SURFACE)),
        area,
    );
}

fn screen_help(screen: Screen) -> Vec<Line<'static>> {
    match screen {
        Screen::Dashboard => vec![
            shortcut("↑/↓ · PgUp/PgDn", "move through recent operations"),
            shortcut("Home / End", "first / last operation"),
            shortcut("X", "cancel selected operation when supported"),
        ],
        Screen::Models => vec![
            shortcut("↑/↓ · PgUp/PgDn", "move through models"),
            shortcut("Home / End", "first / last model"),
            shortcut("mouse wheel", "scroll models"),
            shortcut("Enter / click", "download, load, or unload selected model"),
            shortcut("/", "search Hugging Face"),
            shortcut("i", "show or hide incompatible models"),
            shortcut("d", "download"),
            shortcut("l", "load"),
            shortcut("u", "unload"),
            shortcut("r", "remove"),
            shortcut("x", "cancel active model operation"),
            shortcut("f", "toggle forced load in the load dialog"),
        ],
        Screen::Chat => vec![
            shortcut("Enter", "send message"),
            shortcut("drop image file", "attach image to each prompt"),
            shortcut("↑/↓ · PgUp/PgDn", "scroll conversation"),
            shortcut("Home / End", "top / bottom of conversation"),
            shortcut("Ctrl+←/→", "previous / next loaded model"),
            shortcut("Ctrl+K", "clear conversation"),
            shortcut("Ctrl+D", "remove attached image"),
            shortcut("Ctrl+P", "edit one-off generation parameters"),
            shortcut("Ctrl+T / click", "expand or collapse model reasoning"),
            shortcut("X", "cancel active generation"),
        ],
        Screen::Settings => vec![
            shortcut("↑/↓ · PgUp/PgDn", "move through settings"),
            shortcut("Home / End", "first / last setting"),
            shortcut("Enter / click", "select or edit a setting"),
            shortcut("t", "test Hugging Face token"),
            shortcut("r", "remove selected secret"),
            shortcut("v", "toggle settings table / raw TOML"),
        ],
    }
}

fn shortcut(key: &str, action: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<22}"),
            Style::new().fg(theme::GLACIER).add_modifier(Modifier::BOLD),
        ),
        Span::styled(action.to_owned(), Style::new().fg(theme::INK)),
    ])
}

const fn title(screen: Screen) -> &'static str {
    match screen {
        Screen::Dashboard => "DASHBOARD",
        Screen::Models => "MODELS",
        Screen::Chat => "CHAT",
        Screen::Settings => "SETTINGS",
    }
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
