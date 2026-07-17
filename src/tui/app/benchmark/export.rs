use std::time::{SystemTime, UNIX_EPOCH};

use super::App;
use crate::prompt::benchmark::{write_csv, write_json};

impl App {
    pub(super) fn export_benchmark(&mut self) {
        let (Some(directory), Some(report)) =
            (self.benchmark.export_dir.as_ref(), self.benchmark.runs.last())
        else {
            return;
        };
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |value| value.as_secs());
        let name = report
            .model
            .chars()
            .map(|value| {
                if value.is_ascii_alphanumeric() {
                    value
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let base = directory.join(format!("{stamp}-{name}"));
        match write_json(&base.with_extension("json"), report)
            .and_then(|()| write_csv(&base.with_extension("csv"), report))
        {
            Ok(()) => {
                self.benchmark.error = Some(format!("exported {}.json and .csv", base.display()));
            },
            Err(error) => self.benchmark.error = Some(error.to_string()),
        }
    }
}
