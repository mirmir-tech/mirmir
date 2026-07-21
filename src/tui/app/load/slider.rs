use super::{super::App, dialog::FIELD_COUNT};

const RANGES: [(f64, f64, f64, usize); 5] = [
    (1.0, 131_072.0, 256.0, 0),
    (0.0, 2.0, 0.05, 2),
    (0.0, 1.0, 0.01, 2),
    (0.0, 200.0, 1.0, 0),
    (0.5, 2.0, 0.01, 2),
];

impl App {
    pub(super) const fn select_previous_setting(&mut self) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.selected = dialog.selected.saturating_sub(1);
        }
    }

    pub(super) fn select_next_setting(&mut self) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.selected = dialog.selected.saturating_add(1).min(FIELD_COUNT - 1);
        }
    }

    pub(super) fn edit_setting(&mut self, character: Option<char>) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            let field = &mut dialog.fields[dialog.selected];
            if let Some(character) = character {
                field.push(character);
            } else {
                let _removed = field.pop();
            }
        }
    }

    pub(super) fn toggle_force_load(&mut self) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.force = !dialog.force;
            dialog.error = None;
        }
    }

    pub(super) fn adjust_setting(&mut self, direction: i8) {
        let Some(dialog) = self.load_dialog.as_mut() else {
            return;
        };
        let (minimum, maximum, step, precision) = RANGES[dialog.selected];
        let current = dialog.fields[dialog.selected].parse::<f64>().unwrap_or(minimum);
        let value = step.mul_add(f64::from(direction), current).clamp(minimum, maximum);
        dialog.fields[dialog.selected] = format_value(value, precision);
        dialog.error = None;
    }
}

fn format_value(value: f64, precision: usize) -> String {
    if precision == 0 {
        format!("{value:.0}")
    } else {
        format!("{value:.precision$}")
    }
}
