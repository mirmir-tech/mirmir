use super::{History, Telemetry, source::Source};
use crate::error::Result;

/// Captures only host-side telemetry and persistence; it cannot retain a
/// runtime.
#[derive(Clone)]
pub struct Recorder {
    pub(super) source: Source,
    pub(super) telemetry: Telemetry,
    pub(super) history: History,
}

impl Recorder {
    pub const fn new(source: Source, telemetry: Telemetry, history: History) -> Self {
        Self { source, telemetry, history }
    }

    pub fn record(&self) -> Result<()> {
        self.history.record(&self.telemetry.snapshot(self.source.snapshot()))
    }

    pub fn flush(&self) -> Result<()> {
        self.history.flush()
    }
}
