use super::{super::App, dialog::LoadRow};
use crate::rpc::Client;

impl App {
    pub(super) const fn select_previous_setting(&mut self) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.selected = dialog.selected.saturating_sub(1);
        }
    }

    pub(super) fn select_next_setting(&mut self) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.selected =
                dialog.selected.saturating_add(1).min(dialog.row_count().saturating_sub(1));
        }
    }

    pub(super) fn edit_setting(&mut self, character: Option<char>) {
        let Some(dialog) = self.load_dialog.as_mut() else {
            return;
        };
        let LoadRow::Field(index) = dialog.row() else {
            return;
        };
        let field = &mut dialog.fields[index];
        if let Some(character) = character {
            field.push(character);
        } else {
            let _removed = field.pop();
        }
    }

    pub(super) fn toggle_force_load(&mut self) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.force = !dialog.force;
            dialog.error = None;
        }
    }

    pub(super) fn adjust_setting(&mut self, direction: i8, client: &Client) {
        let Some(dialog) = self.load_dialog.as_mut() else {
            return;
        };
        match dialog.row() {
            LoadRow::Context => self.step_context(direction, client),
            LoadRow::Field(index) => {
                let range = dialog.ranges()[index];
                dialog.fields[index] = range.adjust(&dialog.fields[index], direction);
                dialog.error = None;
            },
        }
    }
}
