use std::io::{self, Stdout};

use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::error::Result;

pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal })
    }

    pub fn draw(&mut self, render: impl FnOnce(&mut ratatui::Frame<'_>)) -> Result<()> {
        self.terminal.draw(render)?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _raw_mode = disable_raw_mode();
        let _screen =
            execute!(self.terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen, Show);
        let _cursor = self.terminal.show_cursor();
    }
}
