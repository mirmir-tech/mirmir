use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use hf_hub::progress::{DownloadEvent, ProgressEvent, ProgressHandler};
use tokio::sync::mpsc;

use super::TransferUpdate;

pub struct Reporter {
    sender: mpsc::Sender<TransferUpdate>,
    files: Mutex<HashMap<String, u64>>,
    total: Mutex<Option<u64>>,
    last_emit: Mutex<Instant>,
}

impl Reporter {
    pub fn new(sender: mpsc::Sender<TransferUpdate>) -> Self {
        Self {
            sender,
            files: Mutex::new(HashMap::new()),
            total: Mutex::new(None),
            last_emit: Mutex::new(Instant::now()),
        }
    }

    fn report(&self, phase: &'static str, bytes: u64, total: Option<u64>, message: &str) {
        drop(self.sender.try_send(TransferUpdate {
            phase,
            downloaded_bytes: bytes,
            total_bytes: total,
            message: message.to_owned(),
        }));
    }

    fn report_progress(&self, bytes: u64, total: Option<u64>) {
        let should_emit = self.last_emit.lock().is_ok_and(|mut last| {
            if last.elapsed() < Duration::from_millis(100) {
                false
            } else {
                *last = Instant::now();
                true
            }
        });
        if should_emit {
            let bytes = total.map_or(bytes, |total| bytes.min(total));
            self.report("downloading", bytes, total, "downloading snapshot");
        }
    }
}

impl ProgressHandler for Reporter {
    fn on_progress(&self, event: &ProgressEvent) {
        let ProgressEvent::Download(event) = event else {
            return;
        };
        match event {
            DownloadEvent::Start { total_files, total_bytes } => {
                let (total, changed) =
                    self.total.lock().map_or((Some(*total_bytes), true), |mut total| {
                        let previous = total.unwrap_or(0);
                        let value = previous.max(*total_bytes);
                        *total = Some(value);
                        (Some(value), value > previous)
                    });
                if changed {
                    self.report("downloading", 0, total, &format!("{total_files} files"));
                }
            },
            DownloadEvent::Progress { files } => {
                if let Ok(mut current) = self.files.lock() {
                    for file in files {
                        current.insert(file.filename.clone(), file.bytes_completed);
                    }
                    let bytes = current.values().copied().sum();
                    let total = self.total.lock().ok().and_then(|value| *value);
                    self.report_progress(bytes, total);
                }
            },
            DownloadEvent::AggregateProgress { bytes_completed, total_bytes, .. } => {
                self.report_progress(*bytes_completed, Some(*total_bytes));
            },
            DownloadEvent::Complete => self.report("finalizing", 0, None, "finalizing snapshot"),
        }
    }
}
