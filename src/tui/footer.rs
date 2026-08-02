use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders},
};

use super::{
    app::{App, ChatSettingsStatus, ChatStatus, LoadStatus, Screen},
    theme,
};

type Shortcut = (&'static str, &'static str);

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let available = usize::from(area.width).saturating_sub(2);
    let shortcuts = fit(&candidates(app), available);
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(Style::new().fg(theme::BORDER))
        .title_top(shortcuts);
    frame.render_widget(block, area);
}

fn candidates(app: &App) -> Vec<Shortcut> {
    if app.help_open {
        return vec![("Esc", "close"), ("?", "close"), ("Enter", "close"), ("Ctrl+C", "quit")];
    }
    if app.navigation.exit_dialog {
        return vec![("Esc / n", "stay"), ("Enter / y", "quit"), ("Ctrl+C", "force quit")];
    }
    if app.remove_dialog.is_some() {
        return vec![("Esc / n", "cancel"), ("Enter / y", "remove"), ("Ctrl+C", "quit")];
    }
    if let Some(dialog) = &app.chat_settings_dialog {
        let mut items = vec![("Esc / x", "close")];
        if dialog.status == ChatSettingsStatus::Editing {
            items.extend([
                ("↑/↓ / Tab", "field"),
                ("type", "edit"),
                ("Enter", "apply"),
                ("s", "save default"),
            ]);
        }
        return items;
    }
    if app.configuration_edit.is_some() {
        return vec![("Esc", "cancel"), ("type", "edit"), ("Enter", "save")];
    }
    if app.editing_search {
        return vec![
            ("Esc", "close"),
            ("type", "search"),
            ("↑/↓", "select"),
            ("Enter", "action"),
            ("PgUp/PgDn", "page"),
        ];
    }
    if let Some(dialog) = &app.load_dialog {
        return load(dialog.status);
    }
    let mut items = vec![("Esc", "quit")];
    if app.screen != Screen::Chat || app.chat_input.is_empty() {
        items.push(("?", "help"));
    }
    items.extend(screen(app));
    items.extend([("Tab/⇧Tab", "tabs"), ("F1–F4", "switch")]);
    if app.screen != Screen::Chat {
        items.push(("q", "quit"));
    }
    items.push(("Ctrl+C", "force quit"));
    items
}

fn load(status: LoadStatus) -> Vec<Shortcut> {
    let mut items = vec![("Esc", "quit")];
    match status {
        LoadStatus::Inspecting => items.push(("x", "cancel")),
        LoadStatus::Editing => items.extend([
            ("x", "cancel"),
            ("↑/↓ / Tab", "field"),
            ("←/→ / type", "value"),
            ("Enter", "load"),
            ("f", "force"),
        ]),
        LoadStatus::Loading => {},
    }
    items
}

fn screen(app: &App) -> Vec<Shortcut> {
    match app.screen {
        Screen::Dashboard => vec![("↑/↓", "select"), ("x", "cancel"), ("PgUp/PgDn", "page")],
        Screen::Models => vec![
            ("↑/↓", "select"),
            ("Enter", "action"),
            ("/", "search"),
            ("l/u", "load/unload"),
            ("r", "remove"),
            ("i", "filter"),
            ("x", "cancel"),
        ],
        Screen::Chat if app.chat_status == ChatStatus::Generating => {
            vec![("x", "cancel"), ("↑/↓", "scroll"), ("PgUp/PgDn", "page")]
        },
        Screen::Chat => vec![
            ("Enter", "send"),
            ("↑/↓", "scroll"),
            ("Ctrl+←/→", "model"),
            ("Ctrl+P", "parameters"),
            ("Ctrl+T", "reasoning"),
            ("Ctrl+K", "clear"),
            ("Ctrl+D", "detach"),
        ],
        Screen::Settings => vec![
            ("↑/↓", "select"),
            ("Enter", "edit"),
            ("v", "raw/table"),
            ("t", "test token"),
            ("r", "remove"),
        ],
    }
}

fn fit(items: &[Shortcut], maximum: usize) -> Line<'static> {
    let mut chosen = Vec::new();
    for item in items {
        let mut trial = chosen.clone();
        trial.push(*item);
        if keys(&trial).width() > maximum {
            break;
        }
        chosen = trial;
    }
    keys(&chosen)
}

fn keys(items: &[Shortcut]) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (index, (key, action)) in items.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ", Style::new().fg(theme::BORDER)));
        }
        spans
            .push(Span::styled(*key, Style::new().fg(theme::GLACIER).add_modifier(Modifier::BOLD)));
        spans.push(Span::styled(format!(" {action}"), Style::new().fg(theme::MUTED)));
    }
    Line::from(spans)
}
