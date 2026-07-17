use libmir::models::tokenizer::TextTokenizer;

use super::markers::{MarkerChannel, MarkerRead, ResponseMarkers};
use crate::error::CliError;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResponseChannel {
    Assistant,
    Thought,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResponseSegment {
    channel: ResponseChannel,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChatResponse {
    raw: String,
    segments: Vec<ResponseSegment>,
    structured: bool,
}

impl ChatResponse {
    pub(super) fn from_tokens(tokenizer: &TextTokenizer, tokens: &[u32]) -> Result<Self, CliError> {
        let raw = tokenizer.decode(tokens)?;
        let Some(markers) = ResponseMarkers::from_tokenizer(tokenizer) else {
            return Ok(Self {
                raw: raw.clone(),
                segments: vec![segment(ResponseChannel::Assistant, &raw)],
                structured: false,
            });
        };
        let segments = parse_segments(tokenizer, tokens, markers)?;
        Ok(Self { raw, segments, structured: true })
    }

    pub(super) fn display_text(&self, review: bool) -> &str {
        let fallback = review.then(|| self.thoughts().next()).flatten();
        if let Some(text) = self.final_text().or(fallback) {
            return text;
        }
        if self.structured {
            ""
        } else {
            &self.raw
        }
    }

    pub(super) fn final_text(&self) -> Option<&str> {
        self.segments
            .iter()
            .find(|segment| segment.channel == ResponseChannel::Assistant)
            .map(|segment| segment.text.as_str())
            .filter(|text| !text.trim().is_empty())
    }

    pub(super) fn thoughts(&self) -> impl Iterator<Item = &str> {
        self.segments
            .iter()
            .filter(|segment| segment.channel == ResponseChannel::Thought)
            .map(|segment| segment.text.as_str())
            .filter(|text| !text.trim().is_empty())
    }
}

fn parse_segments(
    tokenizer: &TextTokenizer,
    tokens: &[u32],
    mut markers: ResponseMarkers,
) -> Result<Vec<ResponseSegment>, CliError> {
    let mut state = SegmentState::new();
    for token in tokens {
        match markers.consume(*token) {
            MarkerRead::NotControl => state.tokens.push(*token),
            MarkerRead::Consumed => {},
            MarkerRead::Channel(channel) => {
                state.flush(tokenizer)?;
                state.channel = response_channel(channel);
            },
        }
    }
    state.flush(tokenizer)?;
    Ok(state.segments)
}

fn response_channel(channel: MarkerChannel) -> ResponseChannel {
    match channel {
        MarkerChannel::Assistant => ResponseChannel::Assistant,
        MarkerChannel::Thought => ResponseChannel::Thought,
        MarkerChannel::Other => ResponseChannel::Other(String::new()),
    }
}

fn segment(channel: ResponseChannel, text: &str) -> ResponseSegment {
    ResponseSegment { channel, text: clean_text(text) }
}

fn clean_text(text: &str) -> String {
    text.trim_matches('\n').to_owned()
}

struct SegmentState {
    channel: ResponseChannel,
    tokens: Vec<u32>,
    segments: Vec<ResponseSegment>,
}

impl SegmentState {
    fn new() -> Self {
        Self {
            channel: ResponseChannel::Assistant,
            tokens: Vec::new(),
            segments: Vec::new(),
        }
    }

    fn flush(&mut self, tokenizer: &TextTokenizer) -> Result<(), CliError> {
        if self.tokens.is_empty() {
            return Ok(());
        }
        let text = tokenizer.decode(&self.tokens)?;
        let segment = segment(self.channel.clone(), &text);
        if !segment.text.is_empty() {
            self.segments.push(segment);
        }
        self.tokens.clear();
        Ok(())
    }
}
