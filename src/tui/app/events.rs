use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};

use super::{App, Screen};
use crate::rpc::Client;

impl App {
    pub async fn handle(&mut self, event: Option<&Event>, client: &mut Client) -> bool {
        let Some(event) = event else {
            return true;
        };
        if force_quit(event) {
            return true;
        }
        if let Event::Mouse(mouse) = event {
            if self.help_open {
                if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                    self.help_open = false;
                }
                return false;
            }
            let navigation_available = self.load_dialog.is_none()
                && self.chat_settings_dialog.is_none()
                && self.remove_dialog.is_none()
                && self.configuration_edit.is_none()
                && !self.navigation.exit_dialog;
            if navigation_available
                && matches!(
                    mouse.kind,
                    MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
                )
                && self.handle_workspace_click(mouse.column, mouse.row)
            {
                return false;
            }
            self.handle_content_mouse(*mouse, client).await;
            return false;
        }
        if let Event::Paste(value) = event {
            if self.screen == Screen::Chat {
                self.handle_chat_paste(value);
            }
            return false;
        }
        let Event::Key(key) = event else {
            return false;
        };
        if self.dismiss_help(*key) {
            return false;
        }
        if self.remove_dialog.is_some() {
            if key.kind == KeyEventKind::Press {
                self.handle_remove_key(*key, client);
            }
            return false;
        }
        if self.chat_settings_dialog.is_some() {
            if key.kind == KeyEventKind::Press {
                self.handle_chat_settings_key(*key, client);
            }
            return false;
        }
        if self.configuration_edit.is_some() && key.kind == KeyEventKind::Press {
            self.handle_configuration_edit(*key, client).await;
            return false;
        }
        if self.open_help(*key) {
            return false;
        }
        if self.editing_search {
            self.handle_search_key(*key, client).await;
            return false;
        }
        if let Some(exit) = self.handle_navigation_key(*key) {
            return exit;
        }
        if key.kind != KeyEventKind::Press {
            return false;
        }
        if self.screen != Screen::Chat && self.handle_list_navigation(key.code) {
            if self.screen == Screen::Models {
                self.load_more_if_needed(client);
            }
            return false;
        }
        if self.load_dialog.is_some() {
            self.handle_load_key(*key, client);
            return false;
        }
        if self.screen == Screen::Chat {
            self.handle_chat_key(*key, client);
            return false;
        }
        if self.screen == Screen::Settings {
            self.handle_configuration_key(*key, client).await;
            return false;
        }
        if self.screen == Screen::Dashboard {
            self.handle_activity_key(*key, client).await;
            return false;
        }
        if self.screen == Screen::Models {
            self.handle_models_key(*key, client).await;
        }
        false
    }
}

fn force_quit(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(key)
            if key.kind != KeyEventKind::Release
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c' | 'C'))
    )
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEvent, KeyEventKind};

    use super::*;

    #[test]
    fn control_c_is_an_unconditional_emergency_exit() {
        let pressed = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        let released = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        ));
        assert!(force_quit(&pressed));
        assert!(!force_quit(&released));
    }
}
