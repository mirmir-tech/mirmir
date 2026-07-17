use std::io::{self, IsTerminal, Write};

use libmir::models::tokenizer::TextTokenizer;

use super::ProgressLine;
use crate::{
    chat::markers::{MarkerChannel, MarkerRead, ResponseMarkers},
    error::CliError,
};

const RESET: &str = "\x1b[0m";
const ITALIC: &str = "\x1b[3m";
const THOUGHT: &str = "\x1b[90m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamChannel {
    Assistant,
    Thought,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Disabled,
    Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThoughtStyle {
    Off,
    On,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputState {
    Empty,
    Written,
}

pub struct TokenStreamer {
    mode: OutputMode,
    markers: Option<ResponseMarkers>,
    channel: StreamChannel,
    thought_style: ThoughtStyle,
    output: OutputState,
    review: bool,
    thinking: Option<ProgressLine>,
    hidden_thought: bool,
}

impl TokenStreamer {
    pub fn terminal(tokenizer: &TextTokenizer, review: bool) -> Self {
        let mode = if io::stdout().is_terminal() {
            OutputMode::Color
        } else {
            OutputMode::Disabled
        };
        Self {
            mode,
            markers: ResponseMarkers::from_tokenizer(tokenizer),
            channel: StreamChannel::Assistant,
            thought_style: ThoughtStyle::Off,
            output: OutputState::Empty,
            review,
            thinking: None,
            hidden_thought: false,
        }
    }

    pub fn disabled() -> Self {
        Self {
            mode: OutputMode::Disabled,
            markers: None,
            channel: StreamChannel::Assistant,
            thought_style: ThoughtStyle::Off,
            output: OutputState::Empty,
            review: false,
            thinking: None,
            hidden_thought: false,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.mode != OutputMode::Disabled
    }

    pub fn write_token(&mut self, tokenizer: &TextTokenizer, token: u32) -> Result<(), CliError> {
        if self.mode == OutputMode::Disabled || self.consume_control_token(token) {
            return Ok(());
        }
        if self.channel == StreamChannel::Other || self.hide_thought() {
            return Ok(());
        }
        let text = tokenizer.decode(&[token])?;
        if text.is_empty() {
            return Ok(());
        }
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        self.enter_channel(&mut handle)?;
        write!(handle, "{text}")?;
        handle.flush()?;
        self.output = OutputState::Written;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), CliError> {
        if let Some(mut thinking) = self.thinking.take() {
            thinking.finish()?;
        }
        if self.mode == OutputMode::Disabled || self.output == OutputState::Empty {
            return Ok(());
        }
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        if self.mode == OutputMode::Color && self.thought_style == ThoughtStyle::On {
            write!(handle, "{RESET}")?;
        }
        writeln!(handle)?;
        handle.flush()?;
        Ok(())
    }

    fn consume_control_token(&mut self, token: u32) -> bool {
        let Some(markers) = self.markers.as_mut() else {
            return false;
        };
        match markers.consume(token) {
            MarkerRead::NotControl => false,
            MarkerRead::Consumed => true,
            MarkerRead::Channel(channel) => {
                self.channel = stream_channel(channel);
                if self.hide_thought() {
                    self.hidden_thought = true;
                    self.start_thinking();
                }
                true
            },
        }
    }

    fn enter_channel(&mut self, handle: &mut impl Write) -> Result<(), CliError> {
        match self.channel {
            StreamChannel::Thought
                if self.mode == OutputMode::Color && self.thought_style == ThoughtStyle::Off =>
            {
                write!(handle, "{THOUGHT}{ITALIC}")?;
                self.thought_style = ThoughtStyle::On;
            },
            StreamChannel::Assistant
                if self.mode == OutputMode::Color && self.thought_style == ThoughtStyle::On =>
            {
                write!(handle, "{RESET}")?;
                if self.output == OutputState::Written {
                    writeln!(handle)?;
                    writeln!(handle)?;
                }
                self.thought_style = ThoughtStyle::Off;
            },
            StreamChannel::Assistant if self.hidden_thought => {
                if let Some(mut thinking) = self.thinking.take() {
                    thinking.finish()?;
                }
                writeln!(handle)?;
                self.hidden_thought = false;
            },
            StreamChannel::Assistant | StreamChannel::Thought | StreamChannel::Other => {},
        }
        Ok(())
    }

    fn hide_thought(&self) -> bool {
        self.channel == StreamChannel::Thought && !self.review
    }

    fn start_thinking(&mut self) {
        if self.thinking.is_none() {
            self.thinking = Some(ProgressLine::start("Thinking..."));
        }
    }
}

fn stream_channel(channel: MarkerChannel) -> StreamChannel {
    match channel {
        MarkerChannel::Assistant => StreamChannel::Assistant,
        MarkerChannel::Thought => StreamChannel::Thought,
        MarkerChannel::Other => StreamChannel::Other,
    }
}
