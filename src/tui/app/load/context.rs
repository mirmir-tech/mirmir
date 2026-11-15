use crate::rpc::proto::{self, model_context_choice::Choice};

/// The K/V capacity a load asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextChoice {
    Automatic,
    Tokens(u64),
}

impl ContextChoice {
    fn from_proto(choice: Option<&proto::ModelContextChoice>) -> Self {
        match choice.and_then(|choice| choice.choice) {
            Some(Choice::Tokens(tokens)) => Self::Tokens(tokens),
            Some(Choice::Automatic(_)) | None => Self::Automatic,
        }
    }

    pub const fn to_proto(self) -> proto::ModelContextChoice {
        let choice = match self {
            Self::Automatic => Choice::Automatic(true),
            Self::Tokens(tokens) => Choice::Tokens(tokens),
        };
        proto::ModelContextChoice { choice: Some(choice) }
    }
}

/// The context row of the load dialog: the server's limits and the user's
/// choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextControl {
    pub limits: proto::ModelContext,
    pub choice: ContextChoice,
}

impl ContextControl {
    pub fn new(limits: proto::ModelContext) -> Self {
        let mut control = Self { choice: ContextChoice::Automatic, limits };
        control.choice = match ContextChoice::from_proto(control.limits.saved.as_ref()) {
            ContextChoice::Tokens(tokens) => ContextChoice::Tokens(control.snap(tokens)),
            ContextChoice::Automatic => ContextChoice::Automatic,
        };
        control
    }

    const fn block(&self) -> u64 {
        if self.limits.block_tokens == 0 {
            1
        } else {
            self.limits.block_tokens
        }
    }

    pub fn minimum(&self) -> u64 {
        self.limits.min_tokens.max(1)
    }

    pub fn maximum(&self) -> u64 {
        self.limits.max_tokens.max(self.minimum())
    }

    /// Rounds `tokens` up to whole blocks within the model's limits.
    pub fn snap(&self, tokens: u64) -> u64 {
        tokens
            .div_ceil(self.block())
            .saturating_mul(self.block())
            .clamp(self.minimum(), self.maximum())
    }

    /// The tokens the choice stands for, with automatic sizing held to the
    /// model's limits.
    pub fn tokens(&self) -> u64 {
        match self.choice {
            ContextChoice::Tokens(tokens) => tokens,
            ContextChoice::Automatic => {
                self.limits.automatic_tokens.clamp(self.minimum(), self.maximum())
            },
        }
    }

    /// The most tokens one request can use under this choice.
    pub fn token_limit(&self) -> u64 {
        match self.choice {
            ContextChoice::Tokens(tokens) => tokens,
            ContextChoice::Automatic => self.maximum(),
        }
    }

    /// The minimum, every power of two between, and the maximum, snapped to
    /// blocks.
    pub fn stops(&self) -> Vec<u64> {
        let (minimum, maximum) = (self.minimum(), self.maximum());
        let mut stops = vec![minimum];
        let mut power = minimum.saturating_add(1).next_power_of_two();
        while power < maximum {
            stops.push(self.snap(power));
            power = power.saturating_mul(2);
        }
        stops.push(maximum);
        stops.dedup();
        stops
    }

    /// Moves to the next stop in `direction`; returns whether the choice
    /// changed.
    pub fn step(&mut self, direction: i8) -> bool {
        let current = self.tokens();
        let stops = self.stops();
        let next = if direction > 0 {
            stops.into_iter().find(|stop| *stop > current)
        } else {
            stops.into_iter().rev().find(|stop| *stop < current)
        };
        let next = match (next, self.choice) {
            (Some(next), _) => next,
            (None, ContextChoice::Automatic) => self.snap(current),
            (None, ContextChoice::Tokens(_)) => return false,
        };
        self.choose(ContextChoice::Tokens(next))
    }

    /// Selects `choice`; returns whether it changed.
    pub fn choose(&mut self, choice: ContextChoice) -> bool {
        let changed = self.choice != choice;
        self.choice = choice;
        changed
    }

    /// Where the chosen tokens sit between the limits on a log scale (0–1).
    pub fn ratio(&self) -> f64 {
        let (minimum, maximum) = (float(self.minimum()).ln(), float(self.maximum()).ln());
        if maximum <= minimum {
            return 1.0;
        }
        ((float(self.tokens()).ln() - minimum) / (maximum - minimum)).clamp(0.0, 1.0)
    }
}

fn float(value: u64) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

#[cfg(test)]
pub(super) const fn limits(saved: ContextChoice) -> proto::ModelContext {
    proto::ModelContext {
        block_tokens: 16,
        min_tokens: 16,
        max_tokens: 40_960,
        saved: Some(saved.to_proto()),
        automatic_tokens: 194_128,
        resident_tokens: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_double_between_limits_and_include_the_maximum() {
        let control = ContextControl::new(limits(ContextChoice::Automatic));
        let stops = control.stops();
        assert_eq!(stops.first(), Some(&16));
        assert_eq!(&stops[1..4], &[32, 64, 128]);
        assert_eq!(&stops[stops.len() - 2..], &[32_768, 40_960]);
    }

    #[test]
    fn steps_from_automatic_and_clamps_at_the_limits() {
        let mut control = ContextControl::new(limits(ContextChoice::Automatic));
        assert_eq!(control.choice, ContextChoice::Automatic);
        assert!(control.step(-1));
        assert_eq!(control.choice, ContextChoice::Tokens(32_768));
        assert!(control.step(1));
        assert_eq!(control.choice, ContextChoice::Tokens(40_960));
        assert!(!control.step(1));
        assert!(control.choose(ContextChoice::Tokens(16)));
        assert!(!control.step(-1));
        assert!(control.choose(ContextChoice::Automatic));
        assert!(!control.choose(ContextChoice::Automatic));
    }

    #[test]
    fn snaps_saved_tokens_to_whole_blocks() {
        let control = ContextControl::new(limits(ContextChoice::Tokens(8_190)));
        assert_eq!(control.choice, ContextChoice::Tokens(8_192));
        assert_eq!(control.snap(1), 16);
        assert_eq!(control.snap(1_000_000), 40_960);
        assert_eq!(control.token_limit(), 8_192);
        assert_eq!(ContextChoice::Tokens(8_192).to_proto().choice, Some(Choice::Tokens(8_192)));
    }
}
