use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::{App, Screen};
use crate::rpc::Client;

#[derive(Debug, Clone, Copy)]
pub struct ListView {
    pub area: Rect,
    pub offset: usize,
}

impl ListView {
    pub fn new(area: Rect, selected: usize, count: usize) -> Self {
        let visible = usize::from(area.height.saturating_sub(2)).max(1);
        let offset = selected.saturating_add(1).saturating_sub(visible).min(count);
        Self { area, offset }
    }

    fn row(self, column: u16, row: u16) -> Option<usize> {
        let inside_x = (self.area.x + 1..self.area.right().saturating_sub(1)).contains(&column);
        let inside_y = (self.area.y + 1..self.area.bottom().saturating_sub(1)).contains(&row);
        (inside_x && inside_y).then(|| self.offset + usize::from(row - self.area.y - 1))
    }
}

impl App {
    pub fn set_list_view(&mut self, area: Option<Rect>) {
        self.list_view = area.map(|area| {
            let (selected, count) = self.list_position();
            ListView::new(area, selected, count)
        });
    }

    pub(super) async fn handle_content_mouse(&mut self, mouse: MouseEvent, client: &mut Client) {
        if self.screen == Screen::Chat {
            self.handle_chat_mouse(mouse.kind);
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => self.move_list(-3),
            MouseEventKind::ScrollDown => self.move_list(3),
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(index) = self.list_view.and_then(|view| view.row(mouse.column, mouse.row))
                else {
                    return;
                };
                self.select_list(index);
                match self.screen {
                    Screen::Models => self.activate_selected_model(client).await,
                    Screen::Settings => self.begin_configuration_edit(),
                    Screen::Overview | Screen::Chat | Screen::Activity | Screen::Benchmarks => {},
                }
            },
            _ => {},
        }
    }

    pub(super) fn handle_list_navigation(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => self.move_list(-1),
            KeyCode::Down => self.move_list(1),
            KeyCode::PageUp => self.move_list(-8),
            KeyCode::PageDown => self.move_list(8),
            KeyCode::Home => self.select_list(0),
            KeyCode::End => self.select_list(usize::MAX),
            _ => return false,
        }
        true
    }

    fn move_list(&mut self, delta: isize) {
        let (selected, count) = self.list_position();
        if count == 0 {
            return;
        }
        let next = selected.saturating_add_signed(delta).min(count - 1);
        self.select_list(next);
    }

    fn select_list(&mut self, index: usize) {
        let (_, count) = self.list_position();
        let selected = index.min(count.saturating_sub(1));
        match self.screen {
            Screen::Models if self.searching_models() => self.catalog_selected = selected,
            Screen::Models => self.local_selected = selected,
            Screen::Settings => self.configuration_selected = selected,
            Screen::Activity => self.activity_selected = selected,
            Screen::Overview | Screen::Chat | Screen::Benchmarks => {},
        }
    }

    fn list_position(&self) -> (usize, usize) {
        match self.screen {
            Screen::Models if self.searching_models() => {
                (self.catalog_selected, self.visible_catalog_count())
            },
            Screen::Models => (self.local_selected, self.local_model_count()),
            Screen::Settings => (self.configuration_selected, self.configuration_count()),
            Screen::Activity => (self.activity_selected, self.activities.len()),
            Screen::Overview | Screen::Chat | Screen::Benchmarks => (0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::proto::LocalModelInfo;

    #[test]
    fn list_navigation_supports_pages_and_boundaries() {
        let mut app = App::new(true);
        app.screen = Screen::Models;
        app.local_models = (0..20).map(local_model).collect();
        assert!(app.handle_list_navigation(KeyCode::PageDown));
        assert_eq!(app.local_selected, 8);
        assert!(app.handle_list_navigation(KeyCode::End));
        assert_eq!(app.local_selected, 19);
        assert!(app.handle_list_navigation(KeyCode::Home));
        assert_eq!(app.local_selected, 0);
    }

    #[test]
    fn viewport_maps_visible_mouse_rows_to_model_indices() {
        let view = ListView::new(Rect::new(10, 5, 40, 7), 12, 20);
        assert_eq!(view.offset, 8);
        assert_eq!(view.row(12, 6), Some(8));
        assert_eq!(view.row(12, 10), Some(12));
        assert_eq!(view.row(9, 6), None);
    }

    fn local_model(index: usize) -> LocalModelInfo {
        LocalModelInfo {
            id: format!("model-{index}"),
            repo_id: format!("Test/Model-{index}"),
            revision: "main".to_owned(),
            commit: String::new(),
            path: format!("/models/{index}"),
            state: "available".to_owned(),
            recent_rank: None,
            selector: format!("model-{index}"),
            managed: true,
        }
    }
}
