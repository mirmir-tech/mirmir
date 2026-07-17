use libmir::models::tokenizer::TextTokenizer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MarkerChannel {
    Assistant,
    Thought,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MarkerRead {
    NotControl,
    Consumed,
    Channel(MarkerChannel),
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ResponseMarkers {
    Channel(ChannelMarkers),
    DelimitedThought(DelimitedThoughtMarkers),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ChannelMarkers {
    start: u32,
    end: Option<u32>,
    thought: Option<u32>,
    final_id: Option<u32>,
    waiting_channel: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DelimitedThoughtMarkers {
    start: u32,
    end: u32,
}

impl ResponseMarkers {
    pub(super) fn from_tokenizer(tokenizer: &TextTokenizer) -> Option<Self> {
        ChannelMarkers::from_tokenizer(tokenizer).map(Self::Channel).or_else(|| {
            DelimitedThoughtMarkers::from_tokenizer(tokenizer).map(Self::DelimitedThought)
        })
    }

    pub(super) fn consume(&mut self, token: u32) -> MarkerRead {
        match self {
            Self::Channel(markers) => markers.consume(token),
            Self::DelimitedThought(markers) => markers.consume(token),
        }
    }
}

impl ChannelMarkers {
    fn from_tokenizer(tokenizer: &TextTokenizer) -> Option<Self> {
        Some(Self {
            start: tokenizer.added_token_id("<|channel>")?,
            end: tokenizer.added_token_id("<channel|>"),
            thought: tokenizer.token_id("thought"),
            final_id: tokenizer.token_id("final"),
            waiting_channel: false,
        })
    }

    fn consume(&mut self, token: u32) -> MarkerRead {
        if self.waiting_channel {
            self.waiting_channel = false;
            return MarkerRead::Channel(self.channel(token));
        }
        if token == self.start {
            self.waiting_channel = true;
            return MarkerRead::Consumed;
        }
        if self.end == Some(token) {
            return MarkerRead::Channel(MarkerChannel::Assistant);
        }
        MarkerRead::NotControl
    }

    fn channel(self, token: u32) -> MarkerChannel {
        if self.thought == Some(token) {
            return MarkerChannel::Thought;
        }
        if self.final_id == Some(token) {
            return MarkerChannel::Assistant;
        }
        MarkerChannel::Other
    }
}

impl DelimitedThoughtMarkers {
    fn from_tokenizer(tokenizer: &TextTokenizer) -> Option<Self> {
        Some(Self {
            start: tokenizer.added_token_id("<think>")?,
            end: tokenizer.added_token_id("</think>")?,
        })
    }

    fn consume(self, token: u32) -> MarkerRead {
        if token == self.start {
            return MarkerRead::Channel(MarkerChannel::Thought);
        }
        if token == self.end {
            return MarkerRead::Channel(MarkerChannel::Assistant);
        }
        MarkerRead::NotControl
    }
}
