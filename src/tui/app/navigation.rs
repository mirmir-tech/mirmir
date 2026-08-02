use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use super::{App, Screen};

pub const WORKSPACE_PREFIX: &str = " WORKSPACE  ";
pub const WORKSPACE_TABS: [(Screen, u8, &str, u16); 4] = [
    (Screen::Dashboard, 1, "Dashboard", 12),
    (Screen::Models, 2, "Models", 9),
    (Screen::Chat, 3, "Chat", 7),
    (Screen::Settings, 4, "Settings", 11),
];

const WORKSPACE_PREFIX_WIDTH: u16 = 12;

#[derive(Debug, Clone, Copy, Default)]
pub struct NavigationState {
    pub exit_dialog: bool,
    workspace_area: Option<Rect>,
}

impl App {
    pub(super) fn dismiss_help(&mut self, key: KeyEvent) -> bool {
        if !self.help_open {
            return false;
        }
        if key.kind == KeyEventKind::Press
            && matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?'))
        {
            self.help_open = false;
        }
        true
    }

    pub(super) fn open_help(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('?')
            && self.load_dialog.is_none()
            && self.chat_settings_dialog.is_none()
            && !self.editing_search
            && (self.screen != Screen::Chat || self.chat_input.is_empty())
        {
            self.help_open = true;
            return true;
        }
        false
    }

    pub(super) fn handle_navigation_key(&mut self, key: KeyEvent) -> Option<bool> {
        if key.kind == KeyEventKind::Release {
            return Some(false);
        }
        if self.navigation.exit_dialog {
            return Some(self.handle_exit_dialog(key.code));
        }
        if let Some(screen) = Self::function_key_screen(key.code) {
            self.select_screen(screen);
            return Some(false);
        }
        match key.code {
            KeyCode::Char('q' | 'Q') if self.screen != Screen::Chat => Some(true),
            KeyCode::Tab => {
                self.select_screen(self.next_screen());
                Some(false)
            },
            KeyCode::BackTab => {
                self.select_screen(self.previous_screen());
                Some(false)
            },
            KeyCode::Esc => {
                self.navigation.exit_dialog = true;
                Some(false)
            },
            _ => None,
        }
    }

    pub(super) fn handle_workspace_click(&mut self, column: u16, row: u16) -> bool {
        let Some(area) = self.navigation.workspace_area else {
            return false;
        };
        if !(area.y..area.bottom()).contains(&row) {
            return false;
        }
        let mut left = area.x.saturating_add(1).saturating_add(WORKSPACE_PREFIX_WIDTH);
        for (screen, _, _, width) in WORKSPACE_TABS {
            let right = left + width;
            if (left..right).contains(&column) {
                self.select_screen(screen);
                return true;
            }
            left = right;
        }
        false
    }

    pub(crate) const fn set_workspace_area(&mut self, area: Rect) {
        self.navigation.workspace_area = Some(area);
    }

    pub const fn advance_animation(&mut self) {
        self.animation_tick = self.animation_tick.wrapping_add(1);
    }

    const fn function_key_screen(code: KeyCode) -> Option<Screen> {
        match code {
            KeyCode::F(1) => Some(Screen::Dashboard),
            KeyCode::F(2) => Some(Screen::Models),
            KeyCode::F(3) => Some(Screen::Chat),
            KeyCode::F(4) => Some(Screen::Settings),
            _ => None,
        }
    }

    const fn next_screen(&self) -> Screen {
        match self.screen {
            Screen::Dashboard => Screen::Models,
            Screen::Models => Screen::Chat,
            Screen::Chat => Screen::Settings,
            Screen::Settings => Screen::Dashboard,
        }
    }

    const fn previous_screen(&self) -> Screen {
        match self.screen {
            Screen::Dashboard => Screen::Settings,
            Screen::Models => Screen::Dashboard,
            Screen::Chat => Screen::Models,
            Screen::Settings => Screen::Chat,
        }
    }

    const fn select_screen(&mut self, screen: Screen) {
        self.screen = screen;
        self.editing_search = false;
    }

    const fn handle_exit_dialog(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Enter | KeyCode::Char('y' | 'Y') => true,
            KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                self.navigation.exit_dialog = false;
                false
            },
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    #[test]
    fn function_keys_select_screens() {
        let mut app = App::new(true);
        for (screen, number, _, _) in WORKSPACE_TABS {
            let key = KeyEvent::new(KeyCode::F(number), KeyModifiers::NONE);
            assert_eq!(app.handle_navigation_key(key), Some(false));
            assert_eq!(app.screen, screen);
        }
    }

    #[test]
    fn tab_cycles_in_both_directions() {
        let mut app = App::new(true);
        app.handle_navigation_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(app.screen, Screen::Models);
        app.handle_navigation_key(KeyEvent::from(KeyCode::BackTab));
        assert_eq!(app.screen, Screen::Dashboard);
        app.handle_navigation_key(KeyEvent::from(KeyCode::BackTab));
        assert_eq!(app.screen, Screen::Settings);
    }

    #[test]
    fn clicking_workspace_selects_tab() {
        let mut app = App::new(true);
        app.set_workspace_area(Rect::new(0, 3, 100, 3));
        for row in 3..6 {
            assert!(app.handle_workspace_click(31, row));
            assert_eq!(app.screen, Screen::Models);
        }
        assert!(!app.handle_workspace_click(31, 6));
    }

    #[test]
    fn escape_requires_confirmation() {
        let mut app = App::new(true);
        assert_eq!(app.handle_navigation_key(KeyEvent::from(KeyCode::Esc)), Some(false));
        assert!(app.navigation.exit_dialog);
        assert_eq!(app.handle_navigation_key(KeyEvent::from(KeyCode::Char('n'))), Some(false));
        assert!(!app.navigation.exit_dialog);
    }

    #[test]
    fn repeated_escape_event_opens_exit_confirmation() {
        let mut app = App::new(true);
        let escape =
            KeyEvent::new_with_kind(KeyCode::Esc, KeyModifiers::NONE, KeyEventKind::Repeat);
        assert_eq!(app.handle_navigation_key(escape), Some(false));
        assert!(app.navigation.exit_dialog);
    }

    #[test]
    fn q_quits_outside_chat_input() {
        let mut app = App::new(true);
        assert_eq!(app.handle_navigation_key(KeyEvent::from(KeyCode::Char('q'))), Some(true));
        app.screen = Screen::Chat;
        assert_eq!(app.handle_navigation_key(KeyEvent::from(KeyCode::Char('q'))), None);
    }

    #[test]
    fn question_mark_opens_and_closes_context_help() {
        let mut app = App::new(true);
        let question = KeyEvent::from(KeyCode::Char('?'));
        assert!(app.open_help(question));
        assert!(app.help_open);
        assert!(app.dismiss_help(question));
        assert!(!app.help_open);
    }

    #[test]
    fn question_mark_stays_available_inside_a_started_chat_prompt() {
        let mut app = App::new(true);
        app.screen = Screen::Chat;
        assert!(app.open_help(KeyEvent::from(KeyCode::Char('?'))));
        app.help_open = false;
        app.chat_input = "Is this".to_owned();
        assert!(!app.open_help(KeyEvent::from(KeyCode::Char('?'))));
    }
}
