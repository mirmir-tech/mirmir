use std::{
    io::{self, IsTerminal, Write},
    time::{Duration, Instant},
};

const CLEAR_LINE: &str = "\r\x1b[2K";
const WIDTH: u64 = 24;

#[derive(Debug, Clone, Copy)]
pub enum ProgressUnit {
    Bytes,
    Tokens,
}

pub struct ProgressBar {
    enabled: bool,
    label: String,
    total: u64,
    unit: ProgressUnit,
    started: Instant,
}

impl ProgressBar {
    pub fn start(label: impl Into<String>, total: u64, unit: ProgressUnit) -> Self {
        Self {
            enabled: io::stderr().is_terminal() && total > 0,
            label: label.into(),
            total,
            unit,
            started: Instant::now(),
        }
    }

    pub fn update(&self, current: u64, detail: &str) {
        if !self.enabled {
            return;
        }
        if draw(self, current.min(self.total), detail).is_err() {}
    }

    pub fn finish(&self) {
        if !self.enabled {
            return;
        }
        if clear().is_err() {}
    }
}

fn draw(bar: &ProgressBar, current: u64, detail: &str) -> io::Result<()> {
    let stderr = io::stderr();
    let mut handle = stderr.lock();
    let filled = current.saturating_mul(WIDTH) / bar.total.max(1);
    let empty = WIDTH.saturating_sub(filled);
    let percent = current.saturating_mul(100) / bar.total.max(1);
    let elapsed = bar.started.elapsed();
    let eta = eta(elapsed, current, bar.total);
    write!(
        handle,
        "{CLEAR_LINE}[{}{}] {:>3}% {} {} eta {} {}",
        "#".repeat(usize::try_from(filled).unwrap_or(0)),
        ".".repeat(usize::try_from(empty).unwrap_or(0)),
        percent,
        bar.label,
        value(bar.unit, current, bar.total),
        seconds(eta),
        detail
    )?;
    handle.flush()
}

fn clear() -> io::Result<()> {
    let stderr = io::stderr();
    let mut handle = stderr.lock();
    write!(handle, "{CLEAR_LINE}")?;
    handle.flush()
}

fn eta(elapsed: Duration, current: u64, total: u64) -> Duration {
    if current == 0 || current >= total {
        return Duration::ZERO;
    }
    let elapsed_secs = elapsed.as_secs_f64();
    let total_secs = elapsed_secs * (f64_from_u64(total) / f64_from_u64(current));
    Duration::from_secs_f64((total_secs - elapsed_secs).max(0.0))
}

fn value(unit: ProgressUnit, current: u64, total: u64) -> String {
    match unit {
        ProgressUnit::Bytes => format!("{}/{}", bytes(current), bytes(total)),
        ProgressUnit::Tokens => format!("{current}/{total} tokens"),
    }
}

fn bytes(value: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let value = f64_from_u64(value);
    if value >= GIB {
        return format!("{:.2} GiB", value / GIB);
    }
    format!("{:.1} MiB", value / MIB)
}

fn seconds(duration: Duration) -> String {
    format!("{:.1}s", duration.as_secs_f64())
}

fn f64_from_u64(value: u64) -> f64 {
    value.to_string().parse::<f64>().unwrap_or(0.0)
}
