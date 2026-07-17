use crossterm::event::{Event, KeyEventKind, MouseButton, MouseEventKind};

use super::{App, Screen};
use crate::rpc::Client;

impl App {
    pub async fn handle(&mut self, event: Option<&Event>, client: &mut Client) -> bool {
        let Some(event) = event else {
            return true;
        };
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
                && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
                && self.handle_workspace_click(mouse.column, mouse.row)
            {
                return false;
            }
            self.handle_content_mouse(*mouse, client).await;
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
        if let Some(exit) = self.handle_navigation_key(*key) {
            return exit;
        }
        if key.kind != KeyEventKind::Press {
            return false;
        }
        if !matches!(self.screen, Screen::Chat | Screen::Benchmarks)
            && self.handle_list_navigation(key.code)
        {
            return false;
        }
        if self.load_dialog.is_some() {
            self.handle_load_key(*key, client);
            return false;
        }
        if self.editing_search {
            self.handle_search_key(*key, client).await;
            return false;
        }
        if self.screen == Screen::Chat {
            self.handle_chat_key(*key, client);
            return false;
        }
        if self.screen == Screen::Benchmarks {
            self.handle_benchmark_key(*key, client);
            return false;
        }
        if self.screen == Screen::Settings {
            self.handle_configuration_key(*key, client).await;
            return false;
        }
        if self.screen == Screen::Activity {
            self.handle_activity_key(*key, client).await;
            return false;
        }
        if self.screen == Screen::Models {
            self.handle_models_key(*key, client).await;
        }
        false
    }
}
