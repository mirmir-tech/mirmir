use std::{
    io::{self, IsTerminal, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::error::CliError;

const CLEAR_LINE: &str = "\r\x1b[2K";
const SPINNER_DELAY: Duration = Duration::from_millis(120);
const FRAMES: [&str; 4] = ["|", "/", "-", "\\"];

pub struct ProgressLine {
    state: Option<ProgressState>,
}

struct ProgressState {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl ProgressLine {
    pub fn start(message: impl Into<String>) -> Self {
        if !io::stderr().is_terminal() {
            return Self { state: None };
        }
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let message = message.into();
        let handle = thread::spawn(move || animate(&message, &worker_stop));
        Self {
            state: Some(ProgressState { stop, handle }),
        }
    }

    pub fn finish(&mut self) -> Result<(), CliError> {
        let Some(state) = self.state.take() else {
            return Ok(());
        };
        state.stop.store(true, Ordering::Relaxed);
        if let Err(_panic) = state.handle.join() {
            return clear_progress_line();
        }
        clear_progress_line()
    }
}

impl Drop for ProgressLine {
    fn drop(&mut self) {
        if self.state.is_some() && self.finish().is_err() {}
    }
}

fn animate(message: &str, stop: &AtomicBool) {
    let mut frame = 0;
    while !stop.load(Ordering::Relaxed) {
        if write_progress_frame(FRAMES[frame % FRAMES.len()], message).is_err() {
            return;
        }
        frame += 1;
        thread::sleep(SPINNER_DELAY);
    }
}

fn write_progress_frame(frame: &str, message: &str) -> Result<(), CliError> {
    let stderr = io::stderr();
    let mut handle = stderr.lock();
    write!(handle, "{CLEAR_LINE}{frame} {message}")?;
    handle.flush()?;
    Ok(())
}

fn clear_progress_line() -> Result<(), CliError> {
    let stderr = io::stderr();
    let mut handle = stderr.lock();
    write!(handle, "{CLEAR_LINE}")?;
    handle.flush()?;
    Ok(())
}
