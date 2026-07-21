use std::io::{self, Stdout};

use crossterm::{
    cursor::{Hide, Show},
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
        supports_keyboard_enhancement,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::error::Result;

pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    enhanced_keyboard: bool,
}

impl TerminalGuard {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        let enhanced_keyboard = supports_keyboard_enhancement().unwrap_or(false);
        if enhanced_keyboard {
            execute!(
                stdout,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
        }
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste, Hide)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal, enhanced_keyboard })
    }

    pub fn draw(&mut self, render: impl FnOnce(&mut ratatui::Frame<'_>)) -> Result<()> {
        self.terminal.draw(render)?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _raw_mode = disable_raw_mode();
        if self.enhanced_keyboard {
            let _keyboard = execute!(self.terminal.backend_mut(), PopKeyboardEnhancementFlags);
        }
        let _screen = execute!(
            self.terminal.backend_mut(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen,
            Show
        );
        let _cursor = self.terminal.show_cursor();
    }
}
